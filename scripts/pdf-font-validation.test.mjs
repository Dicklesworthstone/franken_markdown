// Real qpdf/MuPDF/Poppler and frozen actual-generated PDFs. Mutations are made
// only to private copies through qpdf's JSON writer. No renderer/parser doubles
// or new Rust builds; these are historical-engine fixtures, not current proof.
import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { mkdtemp, readFile, symlink, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import test from 'node:test';

const checker = fileURLToPath(new URL('./check-pdf-fonts.py', import.meta.url));
const fixtures = new URL('../tests/fixtures/pdf-fonts/', import.meta.url);
const provenance = JSON.parse(await readFile(new URL('PROVENANCE.json', fixtures), 'utf8'));
const hash = bytes => createHash('sha256').update(bytes).digest('hex');
function command(binary, args, options = {}) {
  const result = spawnSync(binary, args, { encoding: 'utf8', timeout: 15000, ...options });
  assert.equal(result.error, undefined, String(result.error));
  return result;
}
const pythonProbe = command('python3', ['-c', 'import sys; print(sys.executable)']);
assert.equal(pythonProbe.status, 0, pythonProbe.stderr);
const python = pythonProbe.stdout.trim();
const tools = {};
for (const tool of ['qpdf', 'mutool', 'pdftotext']) {
  const result = command('which', [tool]);
  assert.equal(result.status, 0, `Missing actual validation tool: ${tool}`);
  tools[tool] = result.stdout.trim();
}
function check(path, expected = [], options = {}) {
  const args = [checker, path];
  for (const token of expected) args.push('--expect-text', token);
  const result = command(python, args, options);
  return { ...result, report: JSON.parse(result.stdout) };
}
const source = fileURLToPath(new URL('sans.pdf', fixtures));
const exported = command(tools.qpdf, ['--json-output=2', '--decode-level=all', '--json-stream-data=inline', source]);
assert.equal(exported.status, 0, exported.stderr);
const template = JSON.parse(exported.stdout);
const objects = document => document.qpdf[1];
const dict = (document, ref) => objects(document)[`obj:${ref}`].value;
const firstFont = document => Object.values(objects(document)).find(o => o.value?.['/Subtype'] === '/Type0').value;
const descendant = document => dict(document, firstFont(document)['/DescendantFonts'][0]);
const descriptor = document => dict(document, descendant(document)['/FontDescriptor']);
async function mutated(name, change) {
  const root = await mkdtemp(join(tmpdir(), 'fmd-pdf-font-negative-'));
  const document = structuredClone(template);
  change(document);
  const json = join(root, `${name}.json`), pdf = join(root, `${name}.pdf`);
  await writeFile(json, JSON.stringify(document));
  const result = command(tools.qpdf, ['--json-input', json, pdf]);
  assert.equal(result.status, 0, result.stdout + result.stderr);
  const syntax = command(tools.qpdf, ['--check', pdf]);
  assert.equal(syntax.status, 0, 'Planted font/text defect must retain valid PDF syntax: ' + syntax.stdout + syntax.stderr);
  return pdf;
}

for (const family of ['sans', 'serif']) {
  test(`actual historical ${family} PDF passes five embedded faces, ligatures and exact decomposed text`, async () => {
    const path = fileURLToPath(new URL(`${family}.pdf`, fixtures));
    const original = await readFile(path), fixture = provenance.fixtures.find(x => x.path === `${family}.pdf`);
    assert.equal(hash(original), fixture.sha256);
    const result = check(path, ['AVATAR', 'affinity', 'office', 'Cafe\u0301', 'Å', 'naïve']);
    assert.equal(result.status, 0, result.stdout + result.stderr);
    assert.equal(result.report.ok, true);
    assert.equal(result.report.inputUnchanged, true);
    assert.equal(result.report.fonts.length, 5);
    assert.ok(result.report.glyphUses > 50);
    assert.ok(result.report.unicodeContinuations > 0);
    assert.ok(result.report.fonts.every(f => f.usedGlyphs > 0 && f.glyphCount > f.usedGlyphs));
    assert.equal(result.report.decoders.Poppler.expectedTokens, 6);
    assert.equal(result.report.decoders.MuPDF.expectedTokens, 6);
    assert.deepEqual(await readFile(path), original);
  });
}

const badCases = [
  ['missing-font', d => { delete descriptor(d)['/FontFile2']; }, /Missing embedded TrueType/],
  ['missing-unicode', d => { delete firstFont(d)['/ToUnicode']; }, /Missing ToUnicode/],
  ['invalid-unicode-stream', d => {
    objects(d)[`obj:${firstFont(d)['/ToUnicode']}`].stream.data = Buffer.from('not a CMap').toString('base64');
  }, /Invalid ToUnicode/],
  ['notdef-used', d => {
    const page = Object.values(objects(d)).find(o => o.value?.['/Type'] === '/Page').value;
    const ref = Array.isArray(page['/Contents']) ? page['/Contents'][0] : page['/Contents'];
    const stream = objects(d)[`obj:${ref}`].stream;
    const text = Buffer.from(stream.data, 'base64').toString('latin1');
    const changed = text.replace(/\[<([0-9A-Fa-f]{4})/, '[<0000');
    assert.notEqual(changed, text);
    stream.data = Buffer.from(changed, 'latin1').toString('base64');
  }, /Invalid\/\.notdef used glyph/],
  ['out-of-range-glyph', d => {
    const page = Object.values(objects(d)).find(o => o.value?.['/Type'] === '/Page').value;
    const ref = Array.isArray(page['/Contents']) ? page['/Contents'][0] : page['/Contents'];
    const stream = objects(d)[`obj:${ref}`].stream;
    const text = Buffer.from(stream.data, 'base64').toString('latin1');
    const changed = text.replace(/\[<([0-9A-Fa-f]{4})/, '[<FFFF');
    assert.notEqual(changed, text);
    stream.data = Buffer.from(changed, 'latin1').toString('base64');
  }, /Invalid\/\.notdef used glyph|mutool (?:diagnostic|exited)/],
  ['wrong-width', d => { descendant(d)['/W'][1][1] += 2; }, /width disagrees/],
  ['width-at-bound', d => { descendant(d)['/W'][1][1] += 1; }, /width disagrees/],
  ['invalid-font-header', d => {
    const stream = objects(d)[`obj:${descriptor(d)['/FontFile2']}`].stream;
    const bytes = Buffer.from(stream.data, 'base64'); bytes.write('BAD!', 0, 'ascii'); stream.data = bytes.toString('base64');
  }, /TrueType sfnt header/],
  ['invalid-metric-count', d => {
    const stream = objects(d)[`obj:${descriptor(d)['/FontFile2']}`].stream;
    const bytes = Buffer.from(stream.data, 'base64');
    let edited = false;
    for (let index = 0; index < bytes.readUInt16BE(4); index++) {
      const entry = 12 + index * 16;
      if (bytes.toString('ascii', entry, entry + 4) === 'maxp') {
        bytes.writeUInt16BE(1, bytes.readUInt32BE(entry + 8) + 4); edited = true;
      }
    }
    assert.equal(edited, true); stream.data = bytes.toString('base64');
  }, /TrueType units\/glyph\/metric counts/],
  ['unsupported-encoding', d => { firstFont(d)['/Encoding'] = '/Identity-V'; }, /require Identity-H/],
  ['no-text', d => {
    for (const object of Object.values(objects(d))) if (object.value?.['/Type'] === '/Page') object.value['/Contents'] = [];
  }, /No actual text glyphs/],
];
for (const [name, change, error] of badCases) {
  test(`valid PDF syntax does not admit ${name}`, async () => {
    const path = await mutated(name, change), before = await readFile(path);
    const result = check(path);
    assert.equal(result.status, 70, result.stdout + result.stderr);
    assert.equal(result.report.ok, false);
    assert.match(result.report.error, error);
    assert.deepEqual(await readFile(path), before);
  });
}

test('subunit width quantization is admitted and measured, not called exact placement', async () => {
  const path = await mutated('quantized-width', d => { descendant(d)['/W'][1][1] += 0.5; });
  const result = check(path);
  assert.equal(result.status, 0, result.stdout + result.stderr);
  assert.ok(result.report.fonts.some(f => f.maxWidthDelta1000Em === 0.5));
  assert.match(result.report.widthQuantizationBound, /not exact placement proof/);
});

test('missing exact logical text is rejected without Unicode normalization', () => {
  const result = check(source, ['Cafe\u0301', 'not present in the document']);
  assert.equal(result.status, 70, result.stdout + result.stderr);
  assert.match(result.report.error, /logical text lacks exact expected token/);
});

for (const missing of ['qpdf', 'mutool', 'pdftotext']) {
  test(`missing ${missing} cannot produce a passing skip`, async () => {
    const root = await mkdtemp(join(tmpdir(), 'fmd-pdf-tools-'));
    for (const [name, path] of Object.entries(tools)) if (name !== missing) await symlink(path, join(root, name));
    const result = check(source, [], { env: { ...process.env, PATH: root } });
    assert.equal(result.status, 66, result.stdout + result.stderr);
    assert.equal(result.report.ok, false);
    assert(result.report.error.includes(missing));
  });
}


test('precomposed tokens are not normalized to match decomposed logical source', () => {
  const result = check(source, ['Café']);
  assert.equal(result.status, 70, result.stdout + result.stderr);
  assert.match(result.report.error, /logical text lacks exact expected token/);
});
