// Compare bridge transport in fresh Node processes, alternating baseline and
// current source in one invocation. This does NOT measure Rust rendering,
// WebKit IPC, Swift file I/O, or device RSS. SHA-256 checks every delivered byte.
// Usage: node scripts/profile-ios-export-transfer.mjs BASELINE_COMMIT_SHA
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
    ? execFileSync('git', ['show', `${baselineRef}:${bridgePath}`], {cwd: root, encoding: 'utf8'})
    : readFileSync(`${root}/${bridgePath}`, 'utf8');
  const script = bridge.match(/<script type="module">([\s\S]*?)<\/script>/)[1]
    .replace("import { createRenderer } from './franken_markdown.js';", 'const {createRenderer} = __engine;');
  const bytes = new Uint8Array(size);
  for (let i = 0; i < size; i++) bytes[i] = (i * 53 + 17) % 256;
  const expectedHash = createHash('sha256').update(bytes).digest('hex');
  const received = createHash('sha256');
  let outputBytes = 0;
  let maxEncodedRawBytes = 0;
  const receive = base64 => {
    const decoded = Buffer.from(base64, 'base64');
    outputBytes += decoded.length;
    received.update(decoded);
  };
  const window = {webkit: {messageHandlers: {frankenBridge: {
    postMessage(message) {
      if (message.type === 'exportPdfResult') receive(message.base64);
    },
  }}}};
  await vm.runInNewContext(`(async () => {${script}})()`, {
    window,
    document: {querySelector: () => ({srcdoc: ''})},
    performance,
    TextEncoder,
    fetch: async () => ({arrayBuffer: async () => new ArrayBuffer(0)}),
    btoa: value => {
      maxEncodedRawBytes = Math.max(maxEncodedRawBytes, value.length);
      return Buffer.from(value, 'binary').toString('base64');
    },
    __engine: {createRenderer: async () => ({
      renderPdf: async () => ({bytes, diagnostics: [], extension: 'pdf', mimeType: 'application/pdf'}),
    })},
  });
  const started = performance.now();
  const cpuStarted = process.cpuUsage();
  const result = await window.frankenExportPdf({requestID: 1, markdown: '# transport only'});
  if (typeof window.frankenReadExportChunk === 'function') {
    const manifest = JSON.parse(result);
    const chunkCount = Math.ceil(manifest.byteLength / manifest.chunkSize);
    for (let index = 0; index < chunkCount; index++) {
      const chunk = JSON.parse(window.frankenReadExportChunk({requestID: 1, index}));
      receive(chunk.base64);
    }
    window.frankenFinishExport({requestID: 1, chunkCount, byteLength: manifest.byteLength});
  }
  const elapsedMS = performance.now() - started;
  const cpu = process.cpuUsage(cpuStarted);
  const actualHash = received.digest('hex');
  if (outputBytes !== size || actualHash !== expectedHash) throw new Error('Transport byte parity failed');
  console.log(JSON.stringify({
    which, inputBytes: size, outputBytes, sha256: actualHash, elapsedMS,
    cpuMS: (cpu.user + cpu.system) / 1000,
    peakRssKiB: process.resourceUsage().maxRSS, maxEncodedRawBytes,
  }));
} else {
  const baselineRef = process.argv[2];
  if (!/^[a-f0-9]{7,40}$/i.test(baselineRef ?? '')) {
    throw new Error('Usage: node scripts/profile-ios-export-transfer.mjs BASELINE_COMMIT_SHA');
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
    candidateBridgeSha256: createHash('sha256').update(readFileSync(`${root}/${bridgePath}`)).digest('hex'),
    scenario: 'Actual old/current iOS JS bridge export transport; generated renderer bytes and native decode/hash simulated in Node. No WebKit/iOS device, Swift file I/O or Rust render measurement.',
    node: process.version, platform: process.platform, arch: process.arch, rows,
  }, null, 2));
}
