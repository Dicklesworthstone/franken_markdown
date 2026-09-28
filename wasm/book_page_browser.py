"""Exercise page authoring in the real publisher HTML, without a native renderer.

Loads the production publisher, collection, geometry validator and proof session
as Blob modules. A recording Worker returns protocol fixtures, NOT rendered PDFs.
Other book.js controllers (library/search/HTML viewer/fonts) are not initialized.
Requires Playwright + Chromium; retains downloads and results in a fresh temp dir.
"""
from __future__ import annotations

import hashlib
import json
import os
from pathlib import Path
import re
import tempfile

from playwright.sync_api import sync_playwright

ROOT = Path(__file__).resolve().parent
IMPORT = re.compile(r"(\b(?:from|import)\s*[\"'])(\.[^\"']+)([\"'])")


def modules() -> dict[str, str]:
    result: dict[str, str] = {}

    def visit(path: Path) -> None:
        path = path.resolve()
        name = path.relative_to(ROOT).as_posix()
        if name in result:
            return
        text = path.read_text(encoding="utf-8")
        result[name] = text
        for match in IMPORT.finditer(text):
            visit(path.parent / match[2])

    for name in ["demo/book_controls.mjs", "demo/book_collection.mjs", "book_pdf_proof.mjs"]:
        visit(ROOT / name)
    return result


BOOT = r"""async sources => {
  const urls = new Map();
  function moduleUrl(path) {
    if (urls.has(path)) return urls.get(path);
    const text = sources[path].replace(/(\b(?:from|import)\s*["'])(\.[^"']+)(["'])/g, (_, before, rel, after) => {
      const base = new URL(path, 'https://module.invalid/');
      return before + moduleUrl(new URL(rel, base).pathname.slice(1)) + after;
    });
    const url = URL.createObjectURL(new Blob([text], {type:'text/javascript'}));
    urls.set(path, url); return url;
  }
  const {createBookCollection} = await import(moduleUrl('demo/book_collection.mjs'));
  const {createBookControls} = await import(moduleUrl('demo/book_controls.mjs'));
  const {createBookPdfProof} = await import(moduleUrl('book_pdf_proof.mjs'));
  const collection = createBookCollection(), requests = [];
  collection.append({chapters:[{path:'a.md',source:'\ufeff# A\r\n\r\n{{#include shared.txt}}\r'}],
    includeSources:[{path:'shared.txt',source:'shared\r\n'}],
    images:[{destination:'x.png',bytes:new Uint8Array([1,2])}]});
  collection.setFonts([{slot:'body-regular',name:'fixture.ttf',bytes:new Uint8Array([3,4])}],collection.revision);
  let terminated = 0;
  // Real Worker/structured clone/cancellation, explicitly not the Rust engine.
  const workerUrl = URL.createObjectURL(new Blob([`onmessage = e => setTimeout(() => postMessage(e.data), 20)`], {type:'text/javascript'}));
  const worker = lane => {
    let pending = null;
    const cancel = () => pending?.(Object.assign(Error('recording worker cancelled'), {code:'EXPORT_CANCELLED'}));
    return {cancel, dispose:cancel, render(files,format,options,{signal}={}) {
      requests.push({lane,files,format,options});
      return new Promise((resolve,reject) => {
        const task = new Worker(workerUrl);
        let done = false;
        const finish = error => {
          if(done) return; done = true; task.terminate(); terminated++;
          signal?.removeEventListener('abort',abort); if(pending === finish) pending = null;
          if(error) {reject(error); return;}
          const bytes = new TextEncoder().encode('%PDF-1.7\nprotocol fixture, not native output');
          resolve({format:'book-'+format,extension:format==='site'?'zip':format,mimeType:'application/pdf',
            bytes,sourceLength:42,blob:()=>new Blob([bytes],{type:'application/pdf'})});
        };
        const abort = () => finish(Object.assign(Error('aborted'),{code:'EXPORT_CANCELLED'}));
        pending = finish; signal?.addEventListener('abort',abort,{once:true});
        task.onmessage = () => finish(); task.onerror = () => finish(Error('recording worker failed'));
        if(signal?.aborted) abort(); else task.postMessage({files,format,options});
      });
    }};
  };
  const controls = createBookControls({root:document,collection,worker:worker('publication'),confirm:text=>window.confirm(text)});
  const proof = createBookPdfProof({collection,controls,worker:worker('proof')});
  window.h = {collection,controls,proof,requests,get terminated(){return terminated;},
    original:JSON.stringify(collection.project()),
    close(){proof.dispose();controls.dispose();for(const url of urls.values())URL.revokeObjectURL(url);URL.revokeObjectURL(workerUrl);}};
}"""


def main() -> None:
    out = Path(tempfile.mkdtemp(prefix="fmd-book-page-"))
    source = modules()
    html = (ROOT / "demo/book.html").read_text(encoding="utf-8")
    html, replaced = re.subn(r'<script type="module" src="\./book.js"></script>', "", html)
    assert replaced == 1, "publisher bootstrap changed; update the scoped browser host"
    findings: list[str] = []
    errors: list[str] = []
    network: list[str] = []

    with sync_playwright() as play:
        browser = play.chromium.launch(executable_path=os.environ.get("CHROMIUM", "/usr/bin/chromium"),
                                      headless=True, args=["--no-sandbox"])
        context = browser.new_context(accept_downloads=True, viewport={"width": 1280, "height": 900})
        page = context.new_page()
        page.on("pageerror", lambda error: errors.append(str(error)))
        page.on("request", lambda request: network.append(request.url) if request.url.startswith(("http:", "https:")) else None)
        page.on("dialog", lambda dialog: dialog.accept())
        page.set_content(html)
        page.evaluate(BOOT, source)

        def check(label: str, expression: str) -> None:
            assert page.evaluate(expression), label
            findings.append(label)

        def choose(name: str, value: str) -> None:
            page.locator("#book-page-" + name).select_option(value)

        def fill(name: str, value: str) -> None:
            page.locator("#book-page-" + name).fill(value)

        def apply() -> None:
            page.locator("#book-page-apply").click()

        check("default page remains absent with the original source-project schema", "h.collection.options.page === undefined && h.collection.project().schemaVersion === 2")
        choose("size", "trade"); choose("orientation", "landscape"); choose("unit", "in")
        for side, value in [("top", ".5"), ("right", ".25"), ("bottom", ".75"), ("left", "1")]:
            fill(side, value)
        check("draft does not mutate committed geometry", "h.collection.options.page === undefined")
        apply()
        check("inches and landscape produce exact physical geometry with named margins", "JSON.stringify(h.collection.options.page) === JSON.stringify({size:{widthPt:648,heightPt:432},margins:{topPt:36,rightPt:18,bottomPt:54,leftPt:72}})")
        page.evaluate("window.savedRevision=h.collection.revision; window.savedPage=JSON.stringify(h.collection.options.page)")
        for _ in range(3):
            for unit in ["mm", "pt", "in"]:
                choose("unit", unit)
        apply()
        check("repeated unit conversion is a no-op", "h.collection.revision === savedRevision && JSON.stringify(h.collection.options.page) === savedPage")
        page.locator("#title").fill("Page-configured book")
        page.evaluate("h.controls.prepare('pdf')")
        check("ordinary publishing forwards paper and all book resources", "JSON.stringify(h.requests.at(-1).options.page) === savedPage && h.requests.at(-1).options.includeSources.length===1 && h.requests.at(-1).options.fontAssets.length===1 && h.requests.at(-1).options.images.length===1")
        page.evaluate("async () => {window.oldProof=await h.proof.render();}")
        choose("size", "a4")
        check("unapplied draft preserves retained proof", "h.proof.current === oldProof")
        apply()
        check("applied paper retires old proof and download", "h.proof.current===null && document.querySelector('#download').hidden")
        page.evaluate("window.savedPage=JSON.stringify(h.collection.options.page);window.savedRevision=h.collection.revision")
        choose("size", "custom"); fill("left", "bad"); apply()
        check("invalid numeric input leaves the current page unchanged", "h.collection.revision===savedRevision && JSON.stringify(h.collection.options.page)===savedPage && document.querySelector('#book-page-status').textContent.includes('INVALID_OPTIONS')")
        choose("unit", "mm")
        check("failed unit conversion retains incomplete input", "document.querySelector('#book-page-unit').value==='in' && document.querySelector('#book-page-left').value==='bad'")
        page.locator("#book-page-discard").click()
        choose("size", "custom"); fill("left", "9999"); apply()
        check("margin budget rejects an unusable content box", "h.collection.revision===savedRevision && JSON.stringify(h.collection.options.page)===savedPage")
        page.locator("#book-page-discard").click()
        page.locator("#save-project").click()
        with page.expect_download() as dl:
            page.locator("#download").click()
        source_file = out / "page-project.json"
        dl.value.save_as(source_file)
        stored = json.loads(source_file.read_text(encoding="utf-8"))
        assert stored["schemaVersion"] == 3 and stored["files"][0]["source"].startswith("\ufeff# A\r\n")
        assert "images" not in stored and "fontAssets" not in stored
        findings.append("real source download retains geometry and exact original BOM/newlines without resource grants")
        choose("size", "default"); apply()
        page.locator("#open-project").set_input_files(source_file)
        page.wait_for_function("h.collection.options.page !== undefined && !h.controls.sourceBusy")
        check("file-picker source restore reloads the page while revoking resource grants", "JSON.stringify(h.collection.options.page)===savedPage && h.collection.images.length===0 && h.collection.fonts.length===0 && document.querySelector('#book-page-size').value==='a4'")
        page.evaluate("() => {h.collection.append({chapters:[],images:[{destination:'x.png',bytes:new Uint8Array([1,2])}]});h.collection.setFonts([{slot:'body-regular',name:'fixture.ttf',bytes:new Uint8Array([3,4])}],h.collection.revision);}")
        page.locator("#save-portable").click()
        page.wait_for_function("document.querySelector('#download').download==='book.fmdbook.bundle.json'")
        with page.expect_download() as dl:
            page.locator("#download").click()
        bundle = out / "page-portable.json"
        dl.value.save_as(bundle)
        choose("size", "default"); apply()
        page.locator("#open-portable").set_input_files(bundle)
        page.wait_for_function("h.collection.options.page !== undefined && !h.controls.sourceBusy")
        check("portable file-picker restore installs page and resource bytes together", "JSON.stringify(h.collection.options.page)===savedPage && h.collection.images.length===1 && h.collection.fonts.length===1")
        # Synchronous apply while the real worker is pending; do not rely on an
        # interaction delay being shorter than its 20ms response.
        page.evaluate("() => {h.proof.cancel(); window.stale=h.proof.render().then(()=>false,e=>e.code==='STALE_SOURCE');const n=document.querySelector('#book-page-size');n.value='trade';n.dispatchEvent(new Event('change'));document.querySelector('#book-page-apply').click();}")
        check("applying page changes cancels pending proof and ignores obsolete results", "async () => await stale && h.proof.current===null")
        page.evaluate("document.querySelector('#chapter-source').dispatchEvent(new CompositionEvent('compositionstart'))")
        check("composition blocks page application", "document.querySelector('#book-page-apply').disabled")
        page.evaluate("document.querySelector('#chapter-source').dispatchEvent(new CompositionEvent('compositionend'))")
        check("composition completion restores page authoring", "!document.querySelector('#book-page-apply').disabled")
        page.set_viewport_size({"width": 390, "height": 844})
        page.locator("#book-page-settings").scroll_into_view_if_needed()
        check("page controls fit a narrow viewport", "() => [...document.querySelectorAll('#book-page-settings input,#book-page-settings select,#book-page-settings button')].every(n=>{const r=n.getBoundingClientRect();return r.left>=0 && r.right<=innerWidth;})")
        page.locator("#book-page-settings").screenshot(path=str(out / "page-setup.png"))
        choose("size", "a4")
        page.evaluate("h.controls.suspend()")
        check("suspension discards drafts without losing committed paper", "document.querySelector('#book-page-size').value==='trade' && h.collection.options.page.size.widthPt===648 && h.collection.fonts.length===0 && h.collection.images.length===0")
        choose("size", "default"); apply()
        check("explicit default reset returns to the legacy native-page path", "h.controls.captureProject().schemaVersion===2 && h.collection.options.page===undefined")
        page.evaluate("h.close()")
        check("disposal disables page controls", "document.querySelector('#book-page-apply').disabled")
        browser.close()

    assert not errors, errors
    assert not network, network
    result = {"checks": findings, "passed": len(findings), "page_errors": errors,
              "http_requests": network, "scope": "Production publisher HTML and page/collection/proof modules; recording Workers, not native rendering; auxiliary book.js controllers are not initialized.",
              "source_sha256": {name: hashlib.sha256(text.encode()).hexdigest() for name, text in source.items()},
              "artifact_directory": str(out)}
    (out / "results.json").write_text(json.dumps(result, indent=2) + "\n", encoding="utf-8")
    print(json.dumps(result, indent=2))


if __name__ == "__main__":
    main()
