"""Real Chromium DOM contract checks, not picker/filesystem or full-WASM proof.

Uses about:blank only. Complete production ES modules execute unmodified via an
inline module and import map; the appended export bridge exposes the controller.
Project host/codec callbacks are explicit contract fixtures. File pickers only
cancel or remain pending; no virtual writable handle emulates successful I/O.
Real disk I/O is covered separately by book_file_session.test.mjs.
"""
import json
import pathlib
import shutil
from playwright.sync_api import sync_playwright

ROOT = pathlib.Path(__file__).resolve().parents[1]
HTML = '''<!doctype html><html><head><base href="https://fmd-tests.invalid/demo/">
<meta http-equiv="Content-Security-Policy" content="default-src 'none'; script-src 'unsafe-inline' blob:">
</head><body><section><h2 id="publish-title">Publish</h2><button id="save-project">Prepare source</button></section>
<label>Source<textarea id="chapter-source"></textarea></label><label>Title<input id="title"></label></body></html>'''
HOST = r'''() => {
  let project = {schemaVersion:2, files:[{path:'guide.md',source:'# Guide\n\nOriginal text.\n'},
    {path:'shared.txt',role:'include',source:'Shared text\n'}], options:{title:'Book',lang:'en'}};
  let revision=1, blocked=false, invalid=false, downloads=[], installs=0, failAfterReplace=false;
  const observers=new Set(), busyObservers=new Set();
  const editor=document.querySelector('#chapter-source'), title=document.querySelector('#title');
  const show=()=>{editor.value=project.files[0].source;title.value=project.options.title};show();
  const notify=()=>{for(const fn of observers)fn()};
  const snapshot=()=>structuredClone(project);
  const checkpoint=()=>JSON.stringify([revision,editor.value,title.value]);
  const collection={get revision(){return revision},project:snapshot,
    subscribe(fn){observers.add(fn);return()=>observers.delete(fn)},
    get images(){throw Error('file UI must not inspect images')},get fonts(){throw Error('file UI must not inspect fonts')}};
  const controls=Object.freeze({get sourceBusy(){return blocked}, get currentChapter(){return revision},checkpoint,
    subscribeSourceState(fn){busyObservers.add(fn);return()=>busyObservers.delete(fn)},
    captureProject(){
      if(invalid)throw Object.assign(Error('Invalid applied settings'),{code:'INVALID_OPTIONS'});
      if(project.files[0].source!==editor.value||project.options.title!==title.value){
        project.files[0].source=editor.value;project.options.title=title.value;revision++;notify();
      }
      return snapshot();
    },
    replaceProject(value,stamp){if(stamp!==checkpoint())throw Error('stale host');project=structuredClone(value);revision++;installs++;show();notify();if(failAfterReplace)throw Error('view update failed')},
    prepareSource(full){if(!full)throw Error('source-project route required');downloads.push(new Blob([JSON.stringify(project)+'\n'],{type:'application/json'}))},
  });
  const codec={normalizeBookProject:structuredClone,
    async readBookProject(file){return JSON.parse(await file.text())},
    serializeBookProject(value){return JSON.stringify(value)+'\n'}};
  const w=new EventTarget();w.confirm=()=>false;
  let ui=null,picks=[],opened=0;
  const mount=(supported)=>{
    ui?.dispose();document.querySelector('#book-files')?.remove();
    if(supported){
      w.showOpenFilePicker=options=>{picks.push(options);return Promise.reject(new DOMException('cancelled','AbortError'))};
      w.showSaveFilePicker=options=>{picks.push(options);return Promise.reject(new DOMException('cancelled','AbortError'))};
    }else{delete w.showOpenFilePicker;delete w.showSaveFilePicker}
    ui=globalThis.createBookFileControls({root:document,controls,collection,window:w,projectCodec:codec,onProjectOpened:()=>opened++});
    return ui;
  };
  globalThis.h={mount,controls,collection,w,editor,title,picks,
    get ui(){return ui},get downloads(){return downloads},get installs(){return installs},get opened(){return opened},
    get observerCount(){return observers.size+busyObservers.size},
    setBlocked(value){blocked=value;for(const fn of busyObservers)fn()},setInvalid(value){invalid=value},failAfterReplace(value){failAfterReplace=value},
    state:()=>snapshot(),key(options={}){const e=new KeyboardEvent('keydown',{key:'s',ctrlKey:true,bubbles:true,cancelable:true,...options});editor.dispatchEvent(e);return e.defaultPrevented},
  };mount(false);
}'''

def main():
    executable = shutil.which("chromium") or shutil.which("google-chrome")
    if not executable:
        raise SystemExit("Chromium unavailable; no browser checks were run.")
    results = []
    with sync_playwright() as p:
        browser = p.chromium.launch(executable_path=executable, headless=True)
        try:
            page = browser.new_page()
            page.set_default_timeout(5000)
            errors, requests = [], []
            page.on("pageerror", lambda error: (errors.append(str(error)), print("PAGE ERROR", str(error), flush=True)))
            page.on("request", lambda request: requests.append(request.url))
            page.set_content(HTML)
            page.evaluate(r"""({core,controller})=>{
              const url=URL.createObjectURL(new Blob([core],{type:'text/javascript'}));
              const map=document.createElement('script');map.type='importmap';
              map.textContent=JSON.stringify({imports:{'https://fmd-tests.invalid/demo/book_file_session.mjs':url}});document.head.append(map);
              const module=document.createElement('script');module.type='module';
              module.textContent=controller+'\nglobalThis.createBookFileControls=createBookFileControls;';document.body.append(module);
            }""", {"core": (ROOT / "demo/book_file_session.mjs").read_text(),
                     "controller": (ROOT / "demo/book_file_controls.mjs").read_text()})
            page.wait_for_function("typeof createBookFileControls==='function'")
            page.evaluate(HOST)
            def check(name, expression):
                passed = bool(page.evaluate(expression))
                results.append({"check": name, "passed": passed})
                print(f"{'PASS' if passed else 'FAIL'} {name}")
                if not passed:
                    raise AssertionError(name)
            check("accessible file panel is inserted without creating a worker", "document.querySelector('#book-file-status').getAttribute('role')==='status' && document.querySelector('#book-files').getAttribute('aria-labelledby')==='book-file-title'")
            check("unsupported pickers stay disabled and the fallback is explained", "document.querySelector('#book-file-open').disabled && document.querySelector('#book-file-save-as').disabled && document.querySelector('#book-file-status').textContent.includes('import/download')")
            page.fill("#chapter-source", "# Guide\n\nUnsaved source 😀\n")
            check("Ctrl+S is captured for a source-project download when file pickers are unavailable", "h.key() && h.downloads.length===1")
            check("fallback captures real textarea edits into a native Blob", "async()=>JSON.parse(await h.downloads[0].text()).files[0].source===h.editor.value")
            check("Alt-modified save binding is left alone", "!h.key({altKey:true})")
            page.evaluate("h.editor.dispatchEvent(new CompositionEvent('compositionstart',{bubbles:true}))")
            check("composition leaves save shortcuts to the editor", "!h.key() && h.downloads.length===1")
            page.evaluate("h.editor.dispatchEvent(new CompositionEvent('compositionend',{bubbles:true}));h.setBlocked(true)")
            check("facade preserves the host's live getters", "h.ui.controls.sourceBusy===true && Object.getOwnPropertyDescriptor(h.ui.controls,'sourceBusy').get!==undefined")
            page.evaluate("h.setBlocked(false);h.setInvalid(true);h.key()")
            check("invalid host capture never prepares a fallback download", "h.downloads.length===1 && document.querySelector('#book-file-status').textContent.includes('INVALID_OPTIONS')")
            page.evaluate("h.setInvalid(false);h.mount(true)")
            check("available picker controls are enabled without granting access", "!document.querySelector('#book-file-open').disabled && !document.querySelector('#book-file-save-as').disabled && h.ui.state.filename===null")
            page.click("#book-file-save-as")
            page.wait_for_function("h.ui.state.busy===null && h.picks.length===1")
            check("Save as reaches a JSON-only picker with the book-derived name", "h.picks[0].suggestedName==='Book.fmd-project.json' && h.picks[0].types[0].accept['application/json'][0]==='.json'")
            check("picker cancellation leaves source and file identity intact", "h.ui.state.filename===null && h.installs===0 && document.querySelector('#book-file-status').textContent.includes('FILE_CANCELLED')")
            page.evaluate("h.pendingPicker=new Promise(resolve=>h.resolvePicker=resolve);h.w.showOpenFilePicker=()=>h.pendingPicker;h.pending=h.ui.open();h.pending.catch(()=>{});void 0")
            check("a pending open blocks a competing save", "document.querySelector('#book-file-save-as').disabled && h.ui.state.busy==='Open project'")
            check("pending file access requests a before-unload warning", "(()=>{const e=new Event('beforeunload',{cancelable:true});h.w.dispatchEvent(e);return e.defaultPrevented})()")
            page.evaluate("h.ui.controls.replaceProject(h.state(),h.controls.checkpoint());h.resolvePicker([])")
            page.wait_for_function("h.ui.state.busy===null")
            check("even identical-source recovery retires pending file identity", "async()=>{try{await h.pending;return false}catch(e){return e.code==='STALE_SOURCE' && h.installs===1 && h.ui.state.filename===null && h.ui.state.message.includes('disconnected')}}")
            check("ordinary project methods and getters still operate through the facade", "h.ui.controls.currentChapter===h.controls.currentChapter && JSON.stringify(h.ui.controls.captureProject())===JSON.stringify(h.state())")
            page.evaluate("h.failAfterReplace(true);try{h.ui.controls.replaceProject(h.state(),h.controls.checkpoint())}catch{};h.failAfterReplace(false)")
            check("partial host installation failure also retires old file authority", "h.installs===2 && h.ui.state.message.includes('disconnected')")
            page.evaluate("h.ui.suspend()")
            check("suspension disables new grants and clears file ownership", "document.querySelector('#book-file-open').disabled && h.ui.state.filename===null")
            page.evaluate("h.ui.resume()")
            check("resume does not silently restore file permission", "!document.querySelector('#book-file-open').disabled && h.ui.state.filename===null")
            page.evaluate("h.ui.dispose();h.afterDispose=h.downloads.length")
            check("disposal removes subscriptions and save-key interception", "h.observerCount===0 && !h.key() && h.downloads.length===h.afterDispose && document.querySelector('#book-file-save-as').disabled")
            check("document remains opaque and no filesystem origin was manufactured", "location.href==='about:blank' && self.origin==='null'")
            network = [url for url in requests if not url.startswith("blob:")]
            if errors or network or len(requests) != 1:
                raise AssertionError({"page_errors": errors, "requests": requests})
            print(json.dumps({"checks": len(results), "passed": sum(r["passed"] for r in results),
                "page_errors": errors, "network_requests": network, "blob_module_loads": len(requests), "scope": "Chromium DOM contract; no native picker or filesystem origin"}))
        finally:
            browser.close()

if __name__ == "__main__":
    main()
