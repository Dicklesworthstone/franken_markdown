"""Exercise actual image controls/collection with Files and portable downloads.

Run from the source tree with Playwright and Chromium installed. No Markdown
renderer, PDF viewer, native codec or filesystem-save acceptance is claimed.
Artifacts are retained in a fresh temporary directory, never deleted.
"""
import base64
import json
import os
import tempfile
from pathlib import Path
from playwright.sync_api import sync_playwright

ROOT = Path(__file__).resolve().parent
ART = Path(tempfile.mkdtemp(prefix="fmd-book-images-"))
modules = {name: (ROOT / name).read_text() for name in [
    "pdf_page.mjs", "book_session.mjs", "book_worker.mjs",
    "demo/book_font_assets.mjs", "demo/book_collection.mjs", "demo/book_image_controls.mjs",
]}
# Independent valid one-pixel PNG; pixel decoding is not what this feature does.
png = base64.b64decode("iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO+j8ZkAAAAASUVORK5CYII=")
image = ART / "replacement.png"
image.write_bytes(png)
checks = []
errors = []
requests = []

def check(name, condition):
    checks.append({"check": name, "passed": bool(condition)})
    assert condition, name

with sync_playwright() as p:
    browser = p.chromium.launch(executable_path=os.environ.get("CHROMIUM", "/usr/bin/chromium"), headless=True, args=["--no-sandbox"])
    context = browser.new_context(accept_downloads=True, viewport={"width": 390, "height": 844})
    page = context.new_page()
    page.on("pageerror", lambda error: errors.append(str(error)))
    page.on("request", lambda request: requests.append(request.url) if request.url.startswith(("http:", "https:")) else None)
    page.set_content('<!doctype html><meta charset="utf-8"><title>Book image controls test</title><style>body{font-family:system-ui;padding:12px}select,input,button{max-width:100%;box-sizing:border-box}label{display:block}button{min-height:44px;margin:6px}p{overflow-wrap:anywhere}textarea{width:100%}</style><section><h2 id="arrange-title">Book source and images</h2><textarea id="chapter-source"></textarea></section>')
    page.evaluate("""async sources => {
      const urls = {};
      for (const key of Object.keys(sources)) {
        let code = sources[key];
        for (const [name, url] of Object.entries(urls)) {
          const leaf = name.split('/').pop();
          code = code.replaceAll(`"./${leaf}"`, JSON.stringify(url)).replaceAll(`"../${leaf}"`, JSON.stringify(url));
        }
        urls[key] = URL.createObjectURL(new Blob([code], {type: 'text/javascript'}));
      }
      const {createBookCollection, readPortableBookProject} = await import(urls['demo/book_collection.mjs']);
      const {createBookImageControls, createBookImagePanel} = await import(urls['demo/book_image_controls.mjs']);
      const book = createBookCollection();
      const raw = '\\ufeff# Exact\\r\\n![plot](images/a.png)\\r\\n';
      book.append({chapters: [{path:'book.md', source:raw}], images: [
        {destination:'images/a.png', bytes:new Uint8Array([1,2])},
        {destination:'other.png', bytes:new Uint8Array([3,4])}
      ]});
      const editor = document.querySelector('#chapter-source'); editor.value = raw;
      let busy = false; const listeners = new Set();
      const controls = {
        get sourceBusy(){return busy},
        captureProject(){ book.edit(0, 'book.md', editor.value); return book.project() },
        checkpoint(){ return JSON.stringify([book.revision, editor.value]) },
        subscribeSourceState(fn){listeners.add(fn);return()=>listeners.delete(fn)}
      };
      createBookImagePanel(document);
      const manager = createBookImageControls({root:document, collection:book, controls, confirm:text=>window.confirm(text)});
      editor.addEventListener('input', ()=>controls.captureProject());
      window.f = {book, manager, raw, readPortableBookProject, createBookCollection,
        busy(value){busy=value;for(const fn of listeners)fn()}};
    }""", modules)
    check("two original bindings are listed", page.locator("#book-image-selection option").count() == 2)
    check("replacement needs an explicit file", page.locator("#book-image-replace").is_disabled())
    page.set_input_files("#book-image-file", str(image))
    page.click("#book-image-replace")
    page.wait_for_function("!f.manager.pending")
    check("real File bytes replace the existing binding", page.evaluate("Array.from(f.book.snapshot().options.images[0].bytes)") == list(png))
    check("unrelated image remains", page.evaluate("Array.from(f.book.snapshot().options.images[1].bytes)") == [3, 4])
    check("BOM and CRLF source preserved", page.evaluate("f.book.files[0].source === f.raw"))
    check("file selection is released after success", page.eval_on_selector("#book-image-file", "n=>n.files.length") == 0)
    before = page.evaluate("f.book.revision")
    page.set_input_files("#book-image-file", str(image)); page.click("#book-image-replace")
    page.wait_for_function("!f.manager.pending")
    check("identical replacement is a revision no-op", page.evaluate("f.book.revision") == before)
    with page.expect_download() as capture:
        page.evaluate("""async()=>{ const output=await f.book.portableDownload(); const a=document.createElement('a');
        const url=URL.createObjectURL(output.blob); a.href=url; a.download=output.filename; a.click(); setTimeout(()=>URL.revokeObjectURL(url),1000); }""")
    downloaded = ART / "book.fmdbook.bundle.json"
    capture.value.save_as(downloaded)
    envelope = json.loads(downloaded.read_text())
    check("actual portable download retains replacement bytes", base64.b64decode(envelope["images"][0]["data"]) == png)
    page.once("dialog", lambda dialog: dialog.dismiss())
    page.click("#book-image-remove"); page.wait_for_function("!f.manager.pending")
    check("declining native confirmation preserves image", page.evaluate("f.book.images.length") == 2)
    page.once("dialog", lambda dialog: dialog.accept())
    page.click("#book-image-remove"); page.wait_for_function("!f.manager.pending")
    check("confirming removes only selected binding", page.evaluate("f.book.images.map(i=>i.destination)") == ["other.png"])
    check("removal leaves Markdown unchanged", page.evaluate("f.book.files[0].source===f.raw"))
    page.set_input_files("#book-image-file", str(image))
    page.evaluate("f.busy(true)")
    check("import/composition blocks actions", page.locator("#book-image-replace").is_disabled() and page.locator("#book-image-remove").is_disabled())
    page.evaluate("f.busy(false); f.manager.suspend()")
    check("suspension releases selected File", page.eval_on_selector("#book-image-file", "n=>n.files.length") == 0)
    page.evaluate("f.manager.resume()")
    check("resumption keeps existing source and images", page.evaluate("f.book.images.length===1 && f.book.files[0].source===f.raw"))
    check("mobile controls fit viewport", page.evaluate("document.documentElement.scrollWidth <= innerWidth"))
    page.screenshot(path=str(ART / "images.png"), full_page=True)
    page.evaluate("f.manager.dispose()")
    check("no page errors", not errors)
    check("no HTTP requests", not requests)
    browser.close()

report = {"checks": checks, "pageErrors": errors, "httpRequests": requests,
          "scope": "Production collection/controller; explicit source-host adapter. No native renderer or codec invoked.",
          "localHelperExtracts": "Verbatim input-validation functions" in modules["book_session.mjs"],
          "artifacts": str(ART)}
(ART / "results.json").write_text(json.dumps(report, indent=2))
print(json.dumps(report, indent=2))
