#!/usr/bin/env python3
"""Real Chromium DOM, worker termination and exact-Blob download checks.

Production proof modules; explicit source/editor and worker-transport adapters.
The returned two-page PDF is an independent ReportLab fixture, NOT native WASM
output. This harness does not claim full publisher or typesetting acceptance.
Requires playwright and reportlab; artifacts are retained in a fresh temp dir.
"""
import base64
import hashlib
import io
import json
import os
from pathlib import Path
import tempfile

from playwright.sync_api import sync_playwright
from reportlab.pdfgen import canvas

ROOT = Path(__file__).resolve().parent


def fixture_pdf():
    data = io.BytesIO()
    pdf = canvas.Canvas(data, invariant=1, pageCompression=0)
    for number in [1, 2]:
        pdf.setFont("Helvetica", 22)
        pdf.drawString(48, 740, f"Independent PDF fixture - page {number}")
        pdf.setFont("Helvetica", 12)
        pdf.drawString(48, 704, "Viewer and download lifecycle test; not FrankenMarkdown output.")
        pdf.showPage()
    pdf.save()
    return data.getvalue()


def run():
    output = Path(tempfile.mkdtemp(prefix="fmd-book-pdf-browser-"))
    expected = fixture_pdf()
    (output / "fixture.pdf").write_bytes(expected)
    results = []
    errors = []
    requests = []
    with sync_playwright() as playwright:
        browser = playwright.chromium.launch(
            executable_path=os.environ.get("CHROMIUM", "/usr/bin/chromium"),
            headless=True, args=["--no-sandbox"],
        )
        context = browser.new_context(accept_downloads=True, viewport={"width": 1100, "height": 900})
        page = context.new_page()
        page.on("pageerror", lambda error: errors.append(str(error)))
        page.on("request", lambda request: requests.append(request.url) if request.url.startswith(("http:", "https:")) else None)
        page.set_content('''<!doctype html><html lang="en"><head><title>Book PDF proof fixture</title>
<style>body{font:16px system-ui;padding:20px}section{padding:20px;border:1px solid;margin:20px 0}button,a{margin:8px;padding:12px}textarea{width:100%;height:90px}</style>
</head><body><textarea id="chapter-source"># First</textarea><input id="title" value="Browser proof">
<p><a id="download" href="#ordinary">Unrelated export</a></p>
<section><h2 id="publish-title">Publish</h2><p>Other publishing controls remain independent.</p></section></body></html>''')
        page.evaluate("""async ({core, ui, helper, pdf}) => {
          const moduleUrl = source => URL.createObjectURL(new Blob([source], {type:'text/javascript'}));
          const coreUrl = moduleUrl(core);
          const uiUrl = moduleUrl(ui.replace('"../book_pdf_proof.mjs"', JSON.stringify(coreUrl)));
          const {createBookPdfControls} = await import(uiUrl);
          const {proofHost} = await import(moduleUrl(helper));
          const host = proofHost(); window.proofHost = host;
          const editor = document.querySelector('#chapter-source'), title = document.querySelector('#title');
          const capture = host.controls.captureProject, checkpoint = host.controls.checkpoint;
          host.controls.captureProject = () => {
            host.rawEdit(editor.value);
            if (host.model.options.title !== title.value) host.change(state => state.options.title = title.value);
            return capture();
          };
          host.controls.checkpoint = () => checkpoint() + JSON.stringify([editor.value, title.value]);
          window.workerRecords = []; window.workerDelay = 0;
          const workerUrl = moduleUrl(`onmessage = e => setTimeout(() => {
            const bytes = Uint8Array.from(atob(e.data.pdf), ch => ch.charCodeAt(0));
            postMessage({bytes}, [bytes.buffer]);
          }, e.data.delay)`);
          const worker = {
            render(files, format, options, {signal}) {
              const record = {files, format, options, terminated:false}; workerRecords.push(record);
              return new Promise((resolve, reject) => {
                const endpoint = new Worker(workerUrl); let settled = false;
                const finish = (reason, result) => {
                  if (settled) return; settled = true;
                  signal.removeEventListener('abort', abort);
                  endpoint.terminate(); record.terminated = true;
                  reason ? reject(reason) : resolve(result);
                };
                const abort = () => finish(Object.assign(Error('worker cancelled'), {code:'EXPORT_CANCELLED'}));
                signal.addEventListener('abort', abort, {once:true});
                if (signal.aborted) { abort(); return; }
                endpoint.onmessage = event => finish(null, {format:'book-pdf', mimeType:'application/pdf',
                  extension:'pdf', bytes:event.data.bytes, sourceLength:files.reduce((n,file)=>n+file.source.length,0)});
                endpoint.onerror = event => finish(Error(event.message));
                endpoint.postMessage({pdf, delay:workerDelay, files, options});
              });
            }, dispose() {},
          };
          window.pdfControls = createBookPdfControls({root:document, collection:host.collection,
            controls:host.controls, worker});
        }""", {"core": (ROOT / "book_pdf_proof.mjs").read_text(),
                "ui": (ROOT / "demo/book_pdf_controls.mjs").read_text(),
                "helper": (ROOT / "tests/book_pdf_proof_helpers.mjs").read_text(),
                "pdf": base64.b64encode(expected).decode()})

        def check(name, condition):
            assert condition, name
            results.append({"check": name, "outcome": "PASS"})

        check("no automatic render", page.evaluate("workerRecords.length") == 0)
        page.get_by_role("button", name="Generate book PDF proof", exact=True).click()
        link = page.locator("#book-pdf-download")
        link.wait_for(state="visible")
        check("real native-viewer object uses the download URL", page.evaluate("document.querySelector('#book-pdf-viewer').data === document.querySelector('#book-pdf-download').href"))
        check("chapter/include/font/image handoff", page.evaluate("workerRecords[0].files.length === 2 && workerRecords[0].options.includeSources.length === 1 && workerRecords[0].options.fontAssets[0].weight === 450 && workerRecords[0].options.images.length === 1"))
        with page.expect_download() as event:
            link.click()
        download = event.value
        download.save_as(output / "download.pdf")
        check("actual downloaded bytes equal the fixture", (output / "download.pdf").read_bytes() == expected)
        check("captured title names the download", download.suggested_filename == "Browser-proof.pdf")
        page.locator("#book-pdf-build").click()
        page.wait_for_function("document.querySelector('#book-pdf-status').textContent.includes('reused without rendering')")
        check("regenerate reuses the exact PDF", page.evaluate("workerRecords.length") == 1)
        page.screenshot(path=str(output / "proof.png"), full_page=True)
        page.evaluate("document.querySelector('#chapter-source').value = '# Silent change'")
        check("silent DOM edit refuses download", page.evaluate("(() => {const event = new MouseEvent('click',{cancelable:true}); document.querySelector('#book-pdf-download').dispatchEvent(event); return event.defaultPrevented;})()"))
        check("stale viewing is detached", page.locator("#book-pdf-viewport object").count() == 0)
        page.evaluate("workerDelay = 10000")
        page.locator("#book-pdf-build").click()
        page.wait_for_function("workerRecords.length === 2")
        page.locator("#book-pdf-cancel").click()
        check("cancel terminates the real Worker", page.evaluate("workerRecords[1].terminated"))
        check("other export remains unchanged", page.locator("#download").get_attribute("href") == "#ordinary")
        page.evaluate("workerDelay = 0")
        page.locator("#book-pdf-build").click()
        link.wait_for(state="visible")
        page.evaluate("proofHost.change(state => {state.options.images = []})")
        check("resource changes retire the proof", link.is_hidden() and page.locator("#book-pdf-viewport object").count() == 0)
        page.locator("#book-pdf-build").click()
        link.wait_for(state="visible")
        page.evaluate("pdfControls.suspend()")
        check("suspension clears the proof", link.is_hidden())
        before = page.evaluate("workerRecords.length")
        page.evaluate("pdfControls.resume()")
        check("return does not render automatically", page.evaluate("workerRecords.length") == before)
        page.evaluate("document.querySelector('#chapter-source').dispatchEvent(new CompositionEvent('compositionstart'))")
        check("IME composition blocks generation", page.locator("#book-pdf-build").is_disabled())
        page.evaluate("document.querySelector('#chapter-source').dispatchEvent(new CompositionEvent('compositionend'))")
        check("ending composition restores admission", page.locator("#book-pdf-build").is_enabled())
        page.evaluate("pdfControls.dispose()")
        check("dispose removes host subscriptions", page.evaluate("proofHost.listenerCount") == 0)
        check("zero page errors", not errors)
        check("zero HTTP(S) requests", not requests)
        context.close()
        browser.close()
    report = {"scope": "production PDF proof session/UI; source/editor and worker-transport adapters; independent ReportLab PDF, not compiled WASM",
              "checks": results, "pageErrors": errors, "httpRequests": requests,
              "fixtureSha256": hashlib.sha256(expected).hexdigest(), "artifacts": str(output)}
    (output / "results.json").write_text(json.dumps(report, indent=2) + "\n")
    print(json.dumps(report, indent=2))


if __name__ == "__main__":
    run()
