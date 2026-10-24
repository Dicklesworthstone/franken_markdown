import assert from "node:assert/strict";
import test from "node:test";
import { copyFileSync, mkdirSync, mkdtempSync, readFileSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";
import { spawnSync } from "node:child_process";

const root = fileURLToPath(new URL("../../", import.meta.url));
const feature = ["demo/book_edition.mjs", "demo/book_edition_controls.mjs", "BOOK_EDITION.md"];
const read = path => readFileSync(join(root, path), "utf8");

test("package manifest includes the complete edition module closure", () => {
  const manifest = JSON.parse(read("wasm/package.json"));
  for (const path of feature) assert.equal(manifest.files.filter(value => value === path).length, 1, path);
  assert.match(read("wasm/demo/book_edition_controls.mjs"), /from "\.\/book_edition\.mjs"/);
  assert.equal(manifest.exports["./book-worker"].import, "./book-worker.js");
});

for (const name of ["check-wasm-package.sh", "dsr-wasm-package.sh"]) {
  test(`${name}: execute real copy loops, then import and run the copied edition closure`, async () => {
    const source = read(`scripts/${name}`);
    const loops = [...source.matchAll(/^for file in [\s\S]*?^done$/gm)].map(match => match[0]);
    assert.equal(loops.length, 2, "one top-level and one demo copy loop");
    const fixture = mkdtempSync(join(tmpdir(), "fmd-edition-assembly-")), dest = join(fixture, "package");
    mkdirSync(join(dest, "demo"), { recursive: true });
    // Unrelated package files are named fixture placeholders, never executed.
    // The actual feature sources below are copied byte-for-byte, not rewritten.
    for (const loop of loops) {
      const header = loop.slice("for file in ".length, loop.indexOf("; do"));
      const demo = loop.includes('cp "wasm/demo/$file"');
      for (const name of header.replaceAll("\\\n", " ").trim().split(/\s+/)) {
        assert.match(name, /^[a-zA-Z0-9_.-]+$/, "literal package filename");
        const target = join(fixture, "wasm", demo ? "demo" : "", name);
        mkdirSync(dirname(target), { recursive: true });
        const relative = `${demo ? "demo/" : ""}${name}`;
        if (feature.includes(relative)) copyFileSync(join(root, "wasm", relative), target);
        else writeFileSync(target, `fixture-only: ${relative}\n`);
      }
    }
    const run = spawnSync("bash", ["-eu", "-c", loops.join("\n")], { cwd: fixture, encoding: "utf8",
      env: { ...process.env, package_dir: dest, PACKAGE: dest }, timeout: 10000 });
    assert.equal(run.status, 0, run.stderr);
    for (const path of feature)
      assert.deepEqual(readFileSync(join(dest, path)), readFileSync(join(root, "wasm", path)), path);
    const { createBookEditionSession } = await import(pathToFileURL(join(dest, "demo/book_edition.mjs")));
    const view = await import(pathToFileURL(join(dest, "demo/book_edition_controls.mjs")));
    assert.equal(typeof view.createBookEditionControls, "function");
    assert.equal(typeof view.createBookEditionPanel, "function");
    let revoked = 0, disposed = 0;
    const session = createBookEditionSession({ capture: () => ({ files: [], options: {} }), checkpoint: () => "fixture",
      publisher: {
        render: () => Promise.resolve({ format: "book-publication", sourceLength: 0, byteLength: 1,
          outputs: [{ format: "book-pdf", sourceLength: 0, mimeType: "application/pdf",
            blob: () => new Blob(["x"], { type: "application/pdf" }) }] }),
        cancel() {}, dispose() { disposed++; },
      },
      urls: { createObjectURL: () => "blob:fixture", revokeObjectURL() { revoked++; } },
    });
    session.setFormats(["pdf"]); assert.equal((await session.prepare()).downloads[0].filename, "book.pdf");
    session.dispose(); assert.equal(revoked, 1); assert.equal(disposed, 1);
    assert(source.includes("node --test wasm/tests/book_edition*.test.mjs"));
  });
}

// Evaluate the complete application entry as ESM. Components are explicit VM
// bindings; this verifies wiring/failure isolation, not full workbench startup.
const appFixture = String.raw`
import assert from "node:assert/strict";
import vm from "node:vm";
import {readFileSync} from "node:fs";
const failure=process.env.EDITION_FAIL==='1', calls=[], events=new Map(), nodes=new Map();
const document={querySelector(key){if(!nodes.has(key))nodes.set(key,{textContent:''});return nodes.get(key)}};
const window={addEventListener(name,fn){events.set(name,fn)},confirm(){return true}};
const collection={}, ordinary=[], received={};
const component=name=>({suspend(){calls.push(name+'.suspend')},resume(){calls.push(name+'.resume')},dispose(){calls.push(name+'.dispose')},detach(){calls.push(name+'.detach')},reset(){calls.push(name+'.reset')}});
const host=component('source'), facade=Object.assign({},host,{sourceFacade:true});
const publisher={dispose(){calls.push('publisher.dispose')}};
const bindings={
 '../book-worker.js':{createBookWorker(options){const worker={options};ordinary.push(worker);return worker},createBookPublicationWorker(){calls.push('publisher.create');return publisher}},
 './book_collection.mjs':{createBookCollection:()=>collection,normalizeBookProject(){},readBookProject(){},serializeBookProject(){}},
 './book_controls.mjs':{createBookControls(args){received.source=args;return host}},
 './book_file_controls.mjs':{createBookFileControls(args){assert.equal(args.controls,host);return {...component('files'),controls:facade}}},
 './book_directory_controls.mjs':{createBookDirectoryControls(args){assert.equal(args.controls,facade);return component('directory')}},
 './book_edition_controls.mjs':{createBookEditionControls(args){received.edition=args;if(failure)throw new Error('installation fixture');return {...component('edition'),dispose(){calls.push('edition.dispose');publisher.dispose()}}}},
 './book_image_controls.mjs':{createBookImagePanel(){},createBookImageControls:()=>component('images')},
 './book_font_controls.mjs':{createBookFontPanel(){},createBookFontControls:()=>component('fonts')},
 './book_inspection_controls.mjs':{createBookInspectionPanel(){},createBookLinkPanel(){},createBookInspectionControls:()=>component('inspection'),createBookLinkControls:()=>component('links')},
 './book_library_controls.mjs':{createBookLibraryControls(args){assert.equal(args.controls,facade);return component('library')}},
 './book_pdf_controls.mjs':{createBookPdfPanel(){},createBookPdfControls:()=>component('proof'),createBookPublicationControls:()=>component('profile')},
 './book_preview_controls.mjs':{createBookPreviewControls:()=>component('preview')},
 './book_search_controls.mjs':{createBookSearchControls:()=>component('search')},
 './book_source_history.mjs':{createBookSourceHistoryControls:()=>component('history')},
};
const context=vm.createContext({document,window});
const app=new vm.SourceTextModule(readFileSync(process.env.EDITION_APP,'utf8'),{context});
await app.link(specifier=>{const exports=bindings[specifier];assert(exports,'Unbound application import '+specifier);return new vm.SyntheticModule(Object.keys(exports),function(){for(const [key,value]of Object.entries(exports))this.setExport(key,value)},{context})});
await app.evaluate();
assert.equal(received.edition.controls,facade);assert.equal(received.edition.collection,collection);assert.equal(received.edition.publisher,publisher);assert.equal(received.edition.root,document);
assert.equal(calls.filter(value=>value==='publisher.create').length,1);
assert(ordinary.length>1);assert(ordinary.every(worker=>worker!==publisher));
if(failure){assert.equal(calls.filter(value=>value==='publisher.dispose').length,1);assert.match(nodes.get('#book-edition-status').textContent,/Individual PDF\/EPUB\/site exports/)}
events.get('pagehide')({persisted:true});events.get('pageshow')({persisted:true});
if(!failure){assert(calls.indexOf('edition.suspend')<calls.indexOf('source.suspend'));assert(calls.includes('edition.resume'));assert.equal(calls.filter(value=>value==='publisher.create').length,1)}
events.get('pagehide')({persisted:false});
assert.equal(calls.filter(value=>value==='publisher.dispose').length,1);
if(!failure)assert(calls.indexOf('edition.dispose')<calls.indexOf('source.dispose'));
for(const name of ['files','directory','history','profile','images','fonts','proof','links','inspection','search','preview','library','source'])assert(calls.includes(name+'.dispose'),name);
console.log('application edition wiring and lifecycle passed');
`;
for (const fail of [false, true]) {
  test(`application entry: ${fail ? "edition installation failure preserves other components" : "owned publisher, source facade and lifecycle order"}`, () => {
    const result = spawnSync(process.execPath, ["--experimental-vm-modules", "--input-type=module", "-e", appFixture], {
      encoding: "utf8", timeout: 10000,
      env: { ...process.env, EDITION_APP: join(root, "wasm/demo/book.js"), EDITION_FAIL: fail ? "1" : "0" },
    });
    assert.equal(result.status, 0, result.stderr); assert.match(result.stdout, /application edition wiring and lifecycle passed/);
  });
}
