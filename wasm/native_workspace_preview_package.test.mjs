// Packaging/closure tests, not a replacement for a generated WASM package build.
import assert from 'node:assert/strict';
import {test} from 'node:test';
import {mkdtemp,copyFile,readFile,writeFile} from 'node:fs/promises';
import {tmpdir} from 'node:os';
import {join} from 'node:path';
import {pathToFileURL} from 'node:url';
import {createWorkspacePreviewWorker} from './interactive_preview.mjs';
import {fixtureShell,fixtureSource,fixtureBindings,makePreviewFixture} from './native_workspace_preview_fixture.mjs';

function checkWorkspace(html) {
  assert.match(html, /worker-src blob:/);
  assert.match(html, /script-src 'unsafe-inline' 'wasm-unsafe-eval' blob:/);
  assert.doesNotMatch(html, /'unsafe-eval'/);
  assert.ok(html.includes(createWorkspacePreviewWorker.toString()));
  assert.match(html, /fmd-async-preview-v1/);
  assert.equal((html.match(/id="fmd-native-runtime"/g)||[]).length,1);
  const data = JSON.parse(html.split('<script type="application/json" id="fmd-native-runtime">')[1].split('</script>')[0]);
  assert.equal(data.wasm,'AGFzbQEAAAA=');
  assert.equal(data.bindings,fixtureBindings);
  assert.equal(data.images[0].destination,'fixture.png');
  const source = JSON.parse(html.split('<script type="application/json" id="fmd-raw-source">')[1].split('</script>')[0]);
  assert.equal(source,fixtureSource);
}

test('native exporter embeds the worker factory and exact source/resources without another payload', async()=>{
  checkWorkspace(await makePreviewFixture());
});

test('public exporter imports from a fresh package copy and embeds the actual worker factory', async()=>{
  // Retained for inspection. No generated Rust module is substituted silently:
  // only the wrapper/native ABI below is an explicit adapter.
  const dir = await mkdtemp(join(tmpdir(),'fmd-preview-package-'));
  await writeFile(join(dir,'package.json'),'{"type":"module"}');
  for (const name of ['interactive.js','interactive_runtime.mjs','interactive_preview.mjs']) {
    await copyFile(new URL(name,import.meta.url),join(dir,name));
  }
  await writeFile(join(dir,'shell.html'),await fixtureShell());
  await writeFile(join(dir,'franken_markdown.js'),`
    import {readFileSync} from 'node:fs';
    export async function init(bytes){await WebAssembly.instantiate(bytes);}
    export async function renderInteractiveHtml(){
      const text=readFileSync(new URL('./shell.html',import.meta.url),'utf8');
      return {mimeType:'text/html',bytes:new TextEncoder().encode(text),text:()=>text};
    }
  `);
  const {renderOfflineWorkspace} = await import(pathToFileURL(join(dir,'interactive.js')));
  const result = await renderOfflineWorkspace(fixtureSource,
    {bindings:fixtureBindings,wasm:Uint8Array.of(0,97,115,109,1,0,0,0)},
    {pdfImages:[{destination:'fixture.png',bytes:Uint8Array.of(1,2,3)}]});
  checkWorkspace(result.text());
  await writeFile(join(dir,'workspace.html'),result.bytes);
});

test('npm inventory and both package assemblers include the transitive workspace modules', async()=>{
  const manifest=JSON.parse(await readFile(new URL('package.json',import.meta.url),'utf8'));
  const required=['interactive.js','interactive.d.ts','interactive_runtime.mjs','interactive_preview.mjs',
    'native_workspace.js','native_workspace.d.ts','interactive_export.mjs','pdf_page.mjs','NATIVE_PREVIEW.md'];
  for(const name of required) assert.ok(manifest.files.includes(name),`manifest: ${name}`);
  for(const script of ['check-wasm-package.sh','dsr-wasm-package.sh']) {
    const text=await readFile(new URL('../scripts/'+script,import.meta.url),'utf8');
    const loops=[...text.matchAll(/for file in ([\s\S]*?); do\s*cp "wasm\/\$file"/g)].map(match=>match[1]).join(' ');
    for(const name of required) assert.ok(loops.split(/[\s\\]+/).includes(name),`${script}: ${name}`);
  }
});

test('portable worker bootstrap avoids static module startup while retaining module bindings',()=>{
  const source=createWorkspacePreviewWorker.toString();
  assert.doesNotMatch(source,/type: 'module'/);
  assert.match(source,/await import\(url\)/);
  assert.doesNotMatch(source, /importScripts\(|\beval\(/);
});
