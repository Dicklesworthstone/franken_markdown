// Compare native-shaped source IPC through actual baseline/current bridge code.
// Fresh Node children alternate in one invocation. JSON stringify/parse models
// message copies; this is NOT Swift/WebKit IPC, Rust rendering or device RSS.
// Usage: node scripts/profile-ios-source-transfer.mjs BASELINE_COMMIT_SHA
import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
import {spawnSync, execFileSync} from 'node:child_process';
import {fileURLToPath} from 'node:url';
import {createHash} from 'node:crypto';
import {performance} from 'node:perf_hooks';
import vm from 'node:vm';

const root = fileURLToPath(new URL('../', import.meta.url));
const bridgePath = 'ios/Renderer/bridge.html';

if (process.argv[2] === 'probe') {
  const which = process.argv[3];
  const size = Number(process.argv[4]);
  const baselineRef = process.argv[5];
  const bridge = which === 'incumbent'
    ? execFileSync('git', ['show', baselineRef + ':' + bridgePath], {cwd: root, encoding: 'utf8'})
    : readFileSync(root + '/' + bridgePath, 'utf8');
  const script = bridge.match(/<script type="module">([\s\S]*?)<\/script>/)[1]
    .replace("import { createRenderer } from './franken_markdown.js';", 'const {createRenderer} = __engine;');
  // An ASCII fixture makes character and UTF-8 offsets identical without
  // retaining a complete second encoded source buffer in this native simulator.
  // Separate protocol tests cover all UTF-8 scalar splits, BOMs and malformed input.
  const pattern = '# Source transport\n\n"Quoted", \\escaped and **formatted** Markdown.\n';
  const source = pattern.repeat(Math.ceil(size / pattern.length)).slice(0, size);
  const expectedHash = createHash('sha256').update(source).digest('hex');
  let rendererCalls = 0;
  const observe = markdown => {
    assert.equal(Buffer.byteLength(markdown), size);
    assert.equal(createHash('sha256').update(markdown).digest('hex'), expectedHash);
    rendererCalls++;
  };
  const outputBytes = new TextEncoder().encode('<!doctype html><p>Output is outside this source-transport measurement.</p>');
  const render = (markdown, extension = 'html', mimeType = 'text/html') => {
    observe(markdown);
    return {bytes: outputBytes, extension, mimeType, diagnostics: [],
      text: () => '<!doctype html><p>Output is outside this source-transport measurement.</p>'};
  };
  const window = {webkit: {messageHandlers: {frankenBridge: {postMessage() {}}}}};
  await vm.runInNewContext('(async () => {' + script + '\n})()', {
    window, document: {querySelector: () => ({srcdoc: ''})}, performance,
    TextEncoder, TextDecoder, atob, btoa, unescape,
    fetch: async () => ({arrayBuffer: async () => new ArrayBuffer(0)}),
    __engine: {createRenderer: async () => ({
      renderHtml: async markdown => render(markdown),
      renderPdf: async markdown => render(markdown, 'pdf', 'application/pdf'),
      documentStats: async markdown => { observe(markdown); return {}; },
      accessibilityAudit: async markdown => { observe(markdown); return {}; },
      searchIndex: async markdown => { observe(markdown); return {}; },
    })},
  });
  let maxMessageBytes = 0;
  let totalMessageBytes = 0;
  let messageCount = 0;
  let sourceBytesSent = 0;
  const send = async (method, command) => {
    // A fresh parsed object models transfer ownership across the IPC boundary.
    const json = JSON.stringify(command);
    const length = Buffer.byteLength(json);
    maxMessageBytes = Math.max(maxMessageBytes, length);
    totalMessageBytes += length;
    messageCount++;
    return await window[method](JSON.parse(json));
  };
  // Independent scalar CRC matches the native table implementation. Every
  // allocation and base64 encoding here is bounded to one 64 KiB source chunk.
  const crcTable = Uint32Array.from({length: 256}, (_, value) => {
    let crc = value;
    for (let bit = 0; bit < 8; bit++) crc = (crc >>> 1) ^ (crc & 1 ? 0xedb88320 : 0);
    return crc >>> 0;
  });
  const crc32 = bytes => {
    let crc = 0xffffffff;
    for (let index = 0; index < bytes.length; index++) crc = (crc >>> 8) ^ crcTable[(crc ^ bytes[index]) & 255];
    return ((crc ^ 0xffffffff) >>> 0).toString(16).padStart(8, '0');
  };
  const started = performance.now();
  const cpuStarted = process.cpuUsage();
  const usesHandle = typeof window.frankenBeginSource === 'function';
  let uploadMS = 0;
  if (usesHandle) {
    const manifest = {sourceID: 1, byteLength: size, chunkSize: 65_536};
    assert.deepEqual(JSON.parse(await send('frankenBeginSource', manifest)), manifest);
    let index = 0;
    for (let offset = 0; offset < size; offset += 65_536, index++) {
      const bytes = Buffer.from(source.slice(offset, offset + 65_536), 'utf8');
      const chunk = {sourceID: 1, index, byteLength: bytes.length,
        base64: bytes.toString('base64'), checksum: crc32(bytes)};
      const ack = JSON.parse(await send('frankenAppendSourceChunk', chunk));
      assert.deepEqual(ack, {sourceID: 1, index, receivedBytes: offset + bytes.length});
      sourceBytesSent += bytes.length;
    }
    const receipt = {sourceID: 1, byteLength: size, chunkCount: index};
    assert.deepEqual(JSON.parse(await send('frankenFinishSource', receipt)), receipt);
    uploadMS = performance.now() - started;
  }
  const command = fields => {
    if (usesHandle) return {...fields, sourceID: 1};
    sourceBytesSent += size;
    return {...fields, markdown: source};
  };
  const preview = async (requestID, options) => {
    assert.equal(await send('frankenRender', command({requestID, options})), true);
    assert.equal(await send('frankenCommitPreview', requestID), true);
  };
  await preview(1, {font: 'sans'});
  const firstPreviewMS = performance.now() - started;
  const reusedStarted = performance.now();
  await preview(2, {font: 'serif'});
  await preview(3, {font: 'serif', toc: true});
  for (const [method, requestID] of [['frankenExportHtml', 4], ['frankenExportPdf', 5]]) {
    const manifest = JSON.parse(await send(method, command({requestID, options: {font: 'serif'}})));
    assert.equal(manifest.requestID, requestID);
    // Artifact generation and download are deliberately outside this scenario.
    assert.equal(await send('frankenCancelExport', requestID), true);
  }
  JSON.parse(await send('frankenAnalyze', command({})));
  const cachedOperationsMS = performance.now() - reusedStarted;
  const elapsedMS = performance.now() - started;
  const cpu = process.cpuUsage(cpuStarted);
  assert.equal(rendererCalls, 8); // Three previews, two exports, three analysis calls.
  assert.equal(sourceBytesSent, size * (usesHandle ? 1 : 6));
  console.log(JSON.stringify({
    which, inputBytes: size, sourceBytesSent, rendererCalls, sha256: expectedHash,
    uploadMS, firstPreviewMS, cachedOperationsMS, elapsedMS,
    cpuMS: (cpu.user + cpu.system) / 1000,
    peakRssKiB: process.resourceUsage().maxRSS,
    maxMessageBytes, totalMessageBytes, messageCount,
  }));
} else {
  const baselineRef = process.argv[2];
  if (!/^[a-f0-9]{7,40}$/i.test(baselineRef ?? '')) {
    throw new Error('Usage: node scripts/profile-ios-source-transfer.mjs BASELINE_COMMIT_SHA');
  }
  const rows = [];
  for (let run = 0; run < 3; run++) {
    for (const which of ['incumbent', 'candidate']) {
      const result = spawnSync(process.execPath, [
        process.argv[1], 'probe', which, String(32 * 1024 * 1024), baselineRef,
      ], {encoding: 'utf8', timeout: 60_000});
      if (result.status !== 0) throw new Error(result.stderr || result.error || String(result.status));
      rows.push({run, ...JSON.parse(result.stdout)});
    }
  }
  console.log(JSON.stringify({
    baselineRef,
    candidateBridgeSha256: createHash('sha256').update(readFileSync(root + '/' + bridgePath)).digest('hex'),
    scenario: 'Actual old/current iOS JS bridge; 32 MiB ASCII source; initial preview, two option-only previews, HTML/PDF exports, analysis. Native chunk preparation and JSON message copies simulated in Node; SHA-256 checks every renderer input. No WebKit/iOS device, Swift execution or Rust render measurement.',
    node: process.version, platform: process.platform, arch: process.arch, rows,
  }, null, 2));
}

