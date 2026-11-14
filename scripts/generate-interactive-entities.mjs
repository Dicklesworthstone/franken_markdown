// Regenerate the standalone renderer's character references from the native
// table. Run with Node; --check detects drift without writing or network access.
import {readFileSync, writeFileSync} from 'node:fs';

const args = process.argv.slice(2);
if (args.length > 1 || args.length === 1 && args[0] !== '--check') {
  throw new Error('Usage: node scripts/generate-interactive-entities.mjs [--check]');
}
const nativePath = new URL('../src/parse/entities.rs', import.meta.url);
const clientPath = new URL('../src/interactive_renderer.js', import.meta.url);
const native = readFileSync(nativePath, 'utf8');
const client = readFileSync(clientPath, 'utf8');
const body = native.match(/static HTML5_ENTITIES: &\[\(&str, &str\)\] = &\[\n([\s\S]*?)\n\];/)?.[1];
if (body === undefined) throw new Error('Native HTML5 entity table was not found');
const rows = body.split('\n').map(line => {
  const match = line.match(/^\s*\("([A-Za-z0-9]+)", "((?:\\u\{[0-9a-f]+\})+)"\),$/);
  if (!match) throw new Error('Unsupported native entity row: ' + line);
  const scalars = [...match[2].matchAll(/\\u\{([0-9a-f]+)\}/g)].map(part => part[1]);
  return {name: match[1], scalars};
});
if (rows.length !== 2125 || rows.some((row, index) =>
  row.name.length > 31 || row.scalars.length < 1 || row.scalars.length > 2
  || index > 0 && rows[index - 1].name >= row.name
  || row.scalars.some(value => {
    const scalar = parseInt(value, 16);
    return scalar === 0 || scalar > 0x10ffff || scalar >= 0xd800 && scalar <= 0xdfff;
  }))) throw new Error('Expected the complete sorted native HTML5 entity table');

const lines = [];
let line = '';
for (const row of rows) {
  const token = row.name + '=' + row.scalars.join(',');
  if (line.length + token.length + 1 > 108) {
    lines.push(line); line = token;
  } else line += (line ? ' ' : '') + token;
}
if (line) lines.push(line);
const generated = '// BEGIN GENERATED HTML ENTITIES\n'
  + '// Generated from src/parse/entities.rs by scripts/generate-interactive-entities.mjs.\n'
  + '// Native CommonMark table: 2125 case-sensitive semicolon-terminated names.\n'
  + '// Values are hexadecimal Unicode scalars; no DOM or host entity database.\n'
  + 'const FMD_HTML_ENTITIES = new Map(' + String.fromCharCode(96) + '\n'
  + lines.join('\n') + '\n' + String.fromCharCode(96)
  + '.trim().split(/\\s+/).map(entry => {\n'
  + "  const [name, scalars] = entry.split('=');\n"
  + "  return [name, String.fromCodePoint(...scalars.split(',').map(value => parseInt(value, 16)))];\n"
  + '}));\n// END GENERATED HTML ENTITIES\n';
const marker = /\/\/ BEGIN GENERATED HTML ENTITIES\n[\s\S]*?\/\/ END GENERATED HTML ENTITIES\n/g;
const blocks = [...client.matchAll(marker)];
if (blocks.length !== 1) throw new Error('Expected exactly one generated client entity block');
const updated = client.replace(marker, () => generated);
if (args[0] === '--check') {
  if (updated !== client) throw new Error('Client entity table is stale; run this generator without --check');
} else if (updated !== client) writeFileSync(clientPath, updated);
