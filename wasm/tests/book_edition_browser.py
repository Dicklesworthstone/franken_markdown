#!/usr/bin/env python3
"""Real Chromium DOM/Blob-download checks with explicit source/native fixtures.

Complete production controls, edition session and publication coordinator run
through HTTP ESM imports by default. --opaque-dom uses an about:blank fixture
and rebinds only the controls module's one import to a Blob URL; implementation
bodies remain unchanged and real-origin downloads are not tested. No generated WASM, production source codec,
actual worker thread or publication-format conformance is claimed.
"""
from __future__ import annotations
import argparse
from functools import partial
from http.server import SimpleHTTPRequestHandler, ThreadingHTTPServer
import json
import sys
from pathlib import Path
import tempfile
from threading import Thread
from playwright.sync_api import sync_playwright

ROOT = Path(__file__).resolve().parents[2]
FIXTURE = r'''<!doctype html><html lang="en"><meta charset="utf-8">
<title>Edition controls contract fixture</title>
<style>body{font:16px system-ui;margin:24px;max-width:900px}label{display:block;margin:12px 0}button,a{display:inline-block;padding:12px}section{border:1px solid #bbb;padding:16px;margin:16px 0}textarea{width:90%;height:100px}:focus-visible{outline:3px solid orange}</style>
<h1>Edition controls contract fixture</h1>
<label>Chapter path <input id="chapter-path" value="manual.md"></label>
<label>Chapter text <textarea id="chapter-source"># Manual</textarea></label>
<section><h2 id="publish-title">Prepare and download</h2>
<label>Title <input id="title" value="Manual"></label>
<button id="other-export">Ordinary export fixture</button><p id="host-status"></p></section>
<script type="module">
import {createBookEditionControls, createBookEditionPanel} from '/wasm/demo/book_edition_controls.mjs';
import {createBookPublicationClient} from '/wasm/book_publication.mjs';
const el=id=>document.getElementById(id), listeners=new Set(), sourceListeners=new Set();
let revision=1, source='# Manual', title='Manual', path='manual.md', sourceBusy=false;
let mode='auto', snapshots=0, captures=0, current=null, clients=[], pending=[], calls=[];
const live=new Map(), revoked=[];
const notify=()=>{revision++;for(const fn of [...listeners])fn()};
const collection={
 get revision(){return revision},get files(){return path ? [{path,source}] : []},
 subscribe(fn){listeners.add(fn);return()=>listeners.delete(fn)},
 snapshot(){snapshots++;if(window.h?.changeDuringSnapshot)notify();return structuredClone({files:[{path,source}],options:{title,includeSources:[{path:'parts.txt',source:'Included'}],images:[{destination:'cover.png',bytes:new Uint8Array([1,2,3])}],fontAssets:[{slot:'body',bytes:new Uint8Array([4,5])}],metadataEpochSeconds:1700000000}})},
};
const controls={
 get sourceBusy(){return sourceBusy},
 captureProject(){captures++;if(el('chapter-path').value==='invalid/')throw Object.assign(new Error('Incomplete chapter path'),{code:'INVALID_PATH'});
 const next=[el('chapter-source').value,el('title').value,el('chapter-path').value];
 if(JSON.stringify([source,title,path])!==JSON.stringify(next)){[source,title,path]=next;notify()}
 return {files:[{path,source}],options:{title}}},
 checkpoint(){return JSON.stringify([revision,el('chapter-source').value,el('title').value,el('chapter-path').value])},
 subscribeSourceState(fn){sourceListeners.add(fn);return()=>sourceListeners.delete(fn)},
};
const publisher=createBookPublicationClient({captureInput:(files,options)=>structuredClone({files,options}),createClient(){
 const c={revision:0,disposed:0,input:null};clients.push(c);
 const render=(format)=>{calls.push({client:clients.length,format,input:structuredClone(c.input)});
 const result=()=>({format:`book-${format}`,sourceLength:c.input.files[0].source.length,bytes:new TextEncoder().encode(`fixture-${format}`),retainedInputRevision:++c.revision});
 if(mode==='fail'&&format==='epub')throw Object.assign(new Error('Native fixture rejected EPUB'),{code:'NATIVE_FAILED'});
 if(mode==='manual')return new Promise((resolve,reject)=>pending.push({resolve:()=>resolve(result()),reject}));
 return Promise.resolve(result())};
 return {render(files,format,options){c.input=structuredClone({files,options});return render(format)},
 renderSourceUpdate(changes,format,options){if(changes.length||options.expectedRevision!==c.revision)throw new Error('Invalid retained delta');return render(format)},
 dispose(){c.disposed++}};
}});
const urls={createObjectURL(blob){const url=URL.createObjectURL(blob);live.set(url,blob);return url},revokeObjectURL(url){revoked.push(url);live.delete(url);URL.revokeObjectURL(url)}};
const make=()=>createBookEditionControls({root:document,controls,collection,publisher,urls});
current=make();
// Source-host behavior is an explicit fixture; reject invalid edits without a page error.
for(const id of ['chapter-source','chapter-path','title'])el(id).addEventListener('input',()=>{try{controls.captureProject()}catch(e){el('host-status').textContent=e.code}});
let otherClicks=0;el('other-export').addEventListener('click',()=>otherClicks++);
window.h={get ui(){return current},get calls(){return calls},get clients(){return clients},get pending(){return pending.length},get live(){return live.size},get revoked(){return revoked.length},get captures(){return captures},get snapshots(){return snapshots},get subscriptions(){return [listeners.size,sourceListeners.size]},get otherClicks(){return otherClicks},changeDuringSnapshot:false,
 mode(value){mode=value},resolve(){const job=pending.shift();if(!job)throw new Error('No pending export');job.resolve()},reject(){pending.shift().reject(new Error('late native failure'))},
 blobText(){return live.values().next().value.text()},notify,busy(value){sourceBusy=value;for(const fn of [...sourceListeners])fn()},
 remount(){current=make()},duplicate(){try{make();return null}catch(e){return e.code}},panel(){return createBookEditionPanel(document)},
 async prepare(){try{return (await current.prepare()).phase}catch(e){return e.code}},
 input(id,value){el(id).value=value;el(id).dispatchEvent(new Event('input',{bubbles:true}))},
 activate(type='click'){const link=el('book-edition-downloads').querySelector('a');if(!link)throw new Error('No link');const event=new MouseEvent(type,{bubbles:true,cancelable:true});link.dispatchEvent(event);return event.defaultPrevented},
};
</script></html>'''

class Handler(SimpleHTTPRequestHandler):
    def do_GET(self):
        if self.path == "/fixture":
            data = FIXTURE.encode()
            self.send_response(200)
            self.send_header("Content-Type", "text/html; charset=utf-8")
            self.send_header("Content-Length", str(len(data)))
            self.end_headers()
            self.wfile.write(data)
        elif self.path.startswith("/wasm/"):
            super().do_GET()
        else:
            self.send_error(404)
    def log_message(self, *_args):
        pass


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--chromium", default="/usr/bin/chromium")
    parser.add_argument("--artifacts", type=Path)
    parser.add_argument("--opaque-dom", action="store_true", help="Isolated DOM diagnostic, no real-origin startup or downloads")
    args = parser.parse_args()
    artifact_dir = args.artifacts or Path(tempfile.mkdtemp(prefix="fmd-book-edition-"))
    artifact_dir.mkdir(parents=True, exist_ok=True)
    checks, errors, downloads, requests = [], [], [], []
    server = ThreadingHTTPServer(("127.0.0.1", 0), partial(Handler, directory=str(ROOT)))
    thread = Thread(target=server.serve_forever, daemon=True)
    thread.start()
    try:
        with sync_playwright() as playwright:
            browser = playwright.chromium.launch(executable_path=args.chromium, headless=True)
            context = browser.new_context(accept_downloads=True)
            page = context.new_page()
            page.on("pageerror", lambda error: errors.append(str(error)))
            page.on("download", lambda download: downloads.append(download.suggested_filename))
            page.on("request", lambda request: requests.append(request.url))
            def value(expression):
                return page.evaluate(expression)
            def check(name, expression):
                passed = value(expression) if isinstance(expression, str) else bool(expression)
                checks.append({"check": name, "outcome": "PASS" if passed else "FAIL"})
                print(f"{'PASS' if passed else 'FAIL'} {name}", file=sys.stderr, flush=True)
                assert passed, name
            def fresh():
                if args.opaque_dom:
                    # No origin is impersonated and no blocked navigation is retried.
                    # Only the one static dependency specifier is rebound for Blob ESM.
                    markup, script = FIXTURE.split('<script type="module">', 1)
                    script = script.split('</script>', 1)[0]
                    page.set_content(markup)
                    page.evaluate("delete window.h")
                    page.evaluate("""async ({core,view,coordinator,script}) => {
                      const url=source=>URL.createObjectURL(new Blob([source],{type:'text/javascript'}));
                      const a=url(core), b=url(coordinator);
                      const expected='from "./book_edition.mjs";';
                      if(view.split(expected).length!==2)throw new Error('Unexpected module import');
                      const c=url(view.replace(expected,`from "${a}";`));
                      const d=url(script.replace("from '/wasm/demo/book_edition_controls.mjs'",`from "${c}"`).replace("from '/wasm/book_publication.mjs'",`from "${b}"`));
                      try {await import(d)} finally {for(const u of [a,b,c,d])URL.revokeObjectURL(u)}
                    }""", {"core": (ROOT / 'wasm/demo/book_edition.mjs').read_text(),
                            "view": (ROOT / 'wasm/demo/book_edition_controls.mjs').read_text(),
                            "coordinator": (ROOT / 'wasm/book_publication.mjs').read_text(), "script": script})
                else:
                    page.goto(f"http://127.0.0.1:{server.server_port}/fixture")
                page.wait_for_function("window.h !== undefined")
            def prepare():
                page.locator("#book-edition-prepare").click()
                page.wait_for_function("h.ui.state.phase === 'ready'")
            fresh()
            check("mount is inert, formats labelled, panel is unique", "h.calls.length===0 && h.captures===0 && h.live===0 && document.querySelectorAll('#book-edition-panel').length===1 && document.querySelector('#book-edition-formats legend').textContent==='Formats in this edition' && document.querySelectorAll('#book-edition-panel label input').length===3")
            check("duplicate mount is refused without extra subscriptions", "h.duplicate()==='ALREADY_MOUNTED' && JSON.stringify(h.subscriptions)==='[1,1]' && h.panel()===document.querySelector('#book-edition-panel')")
            value("h.mode('manual')")
            page.locator("#book-edition-prepare").click()
            page.wait_for_function("h.pending===1")
            check("busy state keeps ordinary controls available and exposes no links", "h.ui.state.busy && document.querySelector('#book-edition-prepare').disabled && !document.querySelector('#book-edition-cancel').disabled && document.querySelector('#book-edition-downloads').hidden && !document.querySelector('#other-export').disabled")
            check("snapshot includes includes, images, supplied fonts and applied settings", "h.snapshots===1 && h.calls[0].input.options.includeSources[0].source==='Included' && h.calls[0].input.options.images[0].bytes[2]===3 && h.calls[0].input.options.fontAssets[0].bytes[1]===5 && h.calls[0].input.options.metadataEpochSeconds===1700000000")
            for completed in range(2):
                value("h.resolve()")
                page.wait_for_function(f"h.calls.length==={completed + 2} && h.pending===1")
                check(f"no partial download after {completed + 1} native fixture outputs", "h.live===0 && document.querySelectorAll('#book-edition-downloads a').length===0")
            value("h.resolve()")
            page.wait_for_function("h.ui.state.phase==='ready'")
            check("complete edition exposes three separate explicit links and retires native client", "h.live===3 && document.querySelectorAll('#book-edition-downloads a').length===3 && h.clients[0].disposed===1 && document.querySelector('#book-edition-status').getAttribute('role')==='status'")
            check("preparation starts no browser download", not downloads)
            if args.opaque_dom:
                check("edition link owns exact fixture Blob bytes (not a disk-save test)", "h.blobText().then(text=>text==='fixture-pdf')")
            else:
                with page.expect_download() as event:
                    page.get_by_role("link", name="Download book.pdf (11 bytes)").click()
                saved = event.value
                check("explicit PDF link downloads its exact fixture Blob", saved.suggested_filename == "book.pdf" and Path(saved.path()).read_bytes() == b"fixture-pdf")
            page.screenshot(path=str(artifact_dir / "edition-ready.png"), full_page=True)
            before = len(downloads)
            value("document.querySelector('#chapter-source').value='silent raw change'")
            check("auxiliary activation rejects stale raw input and revokes every link", "h.activate('auxclick') && h.live===0 && document.querySelectorAll('#book-edition-downloads a').length===0")
            check("stale activation starts no download", len(downloads) == before)
            value("h.mode('auto')")
            page.locator("#book-edition-epub").uncheck()
            prepare()
            check("subset selection reaches renderer and download group", "h.ui.state.downloads.map(x=>x.format).join(',')==='pdf,site' && h.calls.slice(-2).map(x=>x.format).join(',')==='pdf,site'")
            value("document.querySelector('#book-edition-pdf').checked=false")
            check("silent selection change blocks old link activation", "h.activate() && h.live===0")
            page.locator("#book-edition-site").uncheck()
            check("empty selection disables prepare without rendering", "document.querySelector('#book-edition-prepare').disabled && h.ui.state.formats.length===0")
            page.locator("#book-edition-pdf").check()
            prepare()
            value("h.input('chapter-path','invalid/')")
            check("invalid host input retires links before failed source admission", "h.live===0 && document.querySelector('#host-status').textContent==='INVALID_PATH'")
            check("invalid source reports failure without native work", "h.prepare().then(code=>code==='INVALID_PATH')")
            value("h.input('chapter-path','manual.md')")
            value("h.busy(true)")
            check("pending imports block preparation", "document.querySelector('#book-edition-prepare').disabled && h.prepare().then(code=>code==='BOOK_BUSY')")
            value("h.busy(false)")
            prepare()
            value("document.querySelector('#title').dispatchEvent(new CompositionEvent('compositionstart',{bubbles:true}))")
            check("composition outside the source textarea blocks capture and retires links", "document.querySelector('#book-edition-prepare').disabled && h.live===0")
            check("compositionend stays blocked through its final input task", "(()=>{const title=document.querySelector('#title');title.dispatchEvent(new CompositionEvent('compositionend',{bubbles:true}));const blocked=document.querySelector('#book-edition-prepare').disabled;title.value='Finished';title.dispatchEvent(new InputEvent('input',{bubbles:true,isComposing:false}));return blocked})()")
            page.wait_for_function("!document.querySelector('#book-edition-prepare').disabled")
            prepare()
            check("finished composition is captured without an automatic render", "h.calls.at(-1).input.options.title==='Finished'")
            value("h.notify()")
            check("resource/collection changes revoke all edition URLs", "h.live===0 && h.ui.state.phase==='idle'")
            value("h.changeDuringSnapshot=true")
            check("snapshot-time host changes are fenced before rendering", "h.prepare().then(code=>code==='STALE_SOURCE')")
            value("h.changeDuringSnapshot=false;h.mode('manual')")
            page.locator("#book-edition-prepare").click()
            page.wait_for_function("h.pending===1")
            value("h.ui.suspend()")
            check("suspension cancels private client, hides links and disables controls", "h.ui.state.phase==='suspended' && h.live===0 && h.clients.at(-1).disposed===1 && document.querySelector('#book-edition-prepare').disabled")
            count = value("h.calls.length")
            value("h.ui.resume();h.resolve()")
            check("resume and late completion never republish or recapture", f"h.calls.length==={count} && h.live===0 && h.ui.state.phase==='idle'")
            value("h.mode('auto')")
            prepare()
            page.locator("#other-export").click()
            check("ordinary exports stay independent", "h.otherClicks===1 && h.live===1")
            value("h.ui.dispose();h.ui.dispose()")
            check("disposal removes subscriptions and clears all URLs", "JSON.stringify(h.subscriptions)==='[0,0]' && h.live===0 && h.ui.state.phase==='disposed'")
            check("no stale DOM listener reacts after disposal", "(()=>{document.querySelector('#book-edition-pdf').checked=false;document.querySelector('#book-edition-pdf').dispatchEvent(new Event('change',{bubbles:true}));return h.ui.state.formats.join(',')==='pdf'})()")
            # A disposed publisher is intentionally permanent; use a fresh page.
            fresh(); value("h.mode('fail')")
            page.locator("#book-edition-prepare").click()
            page.wait_for_function("h.ui.state.phase==='error'")
            check("second-format failure is visible and exposes no partial edition", "h.ui.state.errorCode==='NATIVE_FAILED' && h.live===0 && document.querySelector('#book-edition-status').textContent.includes('No edition downloads')")
            value("h.mode('manual')")
            page.locator("#book-edition-prepare").click()
            page.wait_for_function("h.pending===1")
            page.locator("#book-edition-cancel").click()
            value("h.mode('auto')")
            prepare()
            value("h.reject()")
            page.wait_for_timeout(20)
            check("cancelled old worker failure cannot overwrite a newer edition", "h.ui.state.phase==='ready' && h.live===3")
            value("h.ui.dispose()")
            check("zero uncaught browser errors", not errors)
            check("no remote network requests", all(url.startswith('blob:') if args.opaque_dom else url.startswith(f"http://127.0.0.1:{server.server_port}/") for url in requests))
            context.close(); browser.close()
    finally:
        server.shutdown(); server.server_close(); thread.join()
        (artifact_dir / "results.json").write_text(json.dumps({"checks": checks, "pageErrors": errors, "downloads": downloads,
            "mode": "opaque-dom" if args.opaque_dom else "http",
            "scope": "Production implementation bodies; explicit source codec/native endpoint fixtures, no actual WASM or worker thread. Opaque mode does not test real-origin downloads."}, indent=2) + "\n")
    print(f"{len(checks)} browser checks passed; artifacts: {artifact_dir}")

if __name__ == "__main__":
    main()
