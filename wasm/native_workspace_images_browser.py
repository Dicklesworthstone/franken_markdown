"""Real Chromium authoring lifecycle with shipped JS and explicit renderer doubles.

Instantiates an empty WASM module; does NOT prove Rust parsing, native image
rendering or PDF conformance. Clipboard probes deliver synthetic File/DataTransfer events; file drags use
Chromium's input protocol. Neither proves operating-system clipboard gestures.
Default mode tests real file navigation. Pass
--mode content explicitly where browser policy blocks file://; that mode
reparses actual downloaded bytes in fresh pages. Requires Python Playwright
and a locally installed Chromium; never downloads dependencies or artifacts.
"""
import argparse
import asyncio
import base64
import html
import json
import shutil
import struct
import tempfile
import zlib
from pathlib import Path
from playwright.async_api import async_playwright

ROOT = Path(__file__).resolve().parents[1]
SOURCE = '# Browser imports\n\n![existing](old.png)\n\n</script><script>globalThis.sourceRan=true</script> é中😀\n'


def png_bytes():
    def chunk(tag, data):
        return struct.pack('>I', len(data)) + tag + data + struct.pack('>I', zlib.crc32(tag + data))
    return (b'\x89PNG\r\n\x1a\n' + chunk(b'IHDR', struct.pack('>IIBBBBB', 2, 1, 8, 6, 0, 0, 0))
            + chunk(b'IDAT', zlib.compress(b'\x00\xff\x00\x00\xff\x00\xff\x00\xff')) + chunk(b'IEND', b''))


def inert(value):
    return json.dumps(value, ensure_ascii=True).replace('<', '\\u003c')


def fixture(png):
    # Only native ABI functions and the initial shell are doubles. Runtime,
    # controller, picker, image decoder, editing commands and downloads are real.
    bindings = r'''
export default async function({module_or_path}) { await WebAssembly.instantiate(module_or_path); }
function result(text, mimeType) {
  const bytes = new TextEncoder().encode(text);
  return {bytes, mimeType, diagnosticsJson:()=>'[]', free(){bytes.fill(0);}};
}
function images(keys, flat, lengths) {
  let offset = 0;
  return keys.map((key, i) => {
    const bytes = flat.slice(offset, offset + lengths[i]); offset += lengths[i];
    return [key, btoa(String.fromCharCode(...bytes))];
  });
}
const escape = text => text.replace(/[&<>"']/g, c => ({'&':'&amp;','<':'&lt;','>':'&gt;','"':'&quot;',"'":'&#39;'}[c]));
export function renderHtmlConfiguredAdvanced(...a) {
  const resourceImages = images(a[13], a[14], a[15]).filter(([key]) => a[0].includes(key));
  return result('<html><head></head><body><pre id="source">' + escape(a[0]) + '</pre>'
    + resourceImages.map(([key, data]) => '<img alt="' + escape(key) + '" src="data:image/png;base64,' + data + '">').join('')
    + '</body></html>', 'text/html');
}
function pdf(a, page) {
  return result('%PDF-ADAPTER\n' + JSON.stringify({source:a[0], images:images(a[8], a[9], a[10]),
    weights:Array.from(a[16]), scale:a[21], page}), 'application/pdf');
}
export function renderPdfConfiguredMulti(...a) { return pdf(a, null); }
export function renderPdfConfiguredPage(...a) { return pdf(a, Array.from(a.pop())); }
'''
    payload = {'version': 1, 'bindings': bindings, 'wasm': 'AGFzbQEAAAA=',
               'options': {'font': 'serif', 'fontScale': 1.25, 'pageGeometry': [842, 595, 36, 40, 44, 48]},
               'images': [{'destination': 'old.png', 'bytes': base64.b64encode(png).decode()}],
               'fonts': [{'slot': 'body-regular', 'weight': 555, 'bytes': 'AQID'}]}
    runtime = (ROOT / 'wasm/interactive_runtime.mjs').read_text().replace('export function ', 'function ')
    controller = (ROOT / 'src/interactive_controller.js').read_text()
    importer = (ROOT / 'src/interactive_import.js').read_text()
    buttons = ['toggle-view', 'zoom-out', 'zoom-reset', 'zoom-in', 'theme-toggle', 'stats-toggle',
               'insert-image', 'save-markdown', 'save-html', 'export-pdf']
    policy = ("default-src 'none'; script-src 'unsafe-inline' 'wasm-unsafe-eval' blob:; "
              "style-src 'unsafe-inline' data:; img-src data: blob:; font-src data:; "
              "frame-src 'self' about:; base-uri 'none'; form-action 'none'")
    return ('<!DOCTYPE html><html><head><meta http-equiv="Content-Security-Policy" content="' + policy
            + '"><title>Image lifecycle adapter</title><style>textarea{width:90%;height:180px}button{padding:8px}'
            + '.view-read #editor-pane{display:none}</style></head><body>'
            + '<header class="fmd-app-header">' + ''.join('<button id="btn-' + b + '">' + b + '</button>' for b in buttons)
            + '<input id="fmd-image-picker" type="file" accept="image/png,image/jpeg" multiple hidden>'
            + '<span id="view-mode-icon"></span><span id="view-mode-label"></span></header>'
            + '<div id="fmd-app-body" class="view-split"><section id="editor-pane"><div class="fmd-pane-header">'
            + '<span id="source-line-count"></span><span id="fmd-save-status"></span></div>'
            + '<textarea id="fmd-editor">' + html.escape(SOURCE) + '</textarea></section>'
            + '<main id="preview-pane"><div id="fmd-content">Initializing native renderer</div></main></div>'
            + '<div id="stats-drawer">' + ''.join('<span id="stat-' + s + '"></span>' for s in ['words', 'chars', 'read-time', 'readability'])
            + '<button id="btn-stats-close">Close</button></div>'
            + '<script type="application/json" id="fmd-raw-source">' + inert(SOURCE) + '</script>'
            + '<script type="application/json" id="fmd-native-runtime">' + inert(payload) + '</script>'
            + '<script>' + runtime + '\nbootNativeWorkspace(createNativeWorkspaceRenderer);\n'
            + 'function parseMarkdownClient(){throw Error("Unexpected reduced parser");}\n'
            + controller + '\n' + importer + '\n</script></body></html>')


async def run(args):
    artifact_root = ROOT / 'tests/artifacts/native-image-import'
    artifact_root.mkdir(parents=True, exist_ok=True)
    artifacts = Path(tempfile.mkdtemp(prefix='browser.', dir=artifact_root))
    initial = artifacts / 'initial.html'
    png = png_bytes()
    initial.write_text(fixture(png))
    findings, errors, requests = [], [], []

    def check(condition, label):
        if not condition:
            raise AssertionError(label)
        findings.append(label)

    async with async_playwright() as playwright:
        browser = await playwright.chromium.launch(executable_path=args.chromium, headless=True, args=['--no-sandbox'])
        context = await browser.new_context(accept_downloads=True)
        context.on('request', lambda request: requests.append(request.url) if request.url.startswith(('http:', 'https:')) else None)
        await context.route('http://**/*', lambda route: route.abort())
        await context.route('https://**/*', lambda route: route.abort())

        async def open_page(path):
            page = await context.new_page()
            page.on('pageerror', lambda error: errors.append(str(error)))
            page.on('dialog', lambda dialog: dialog.dismiss())
            if args.mode == 'file':
                await page.goto(path.as_uri())
            else:
                await page.set_content(path.read_text())
            await page.wait_for_function('() => window.__fmdNativeRuntime?.ready !== undefined')
            check(await page.evaluate('window.__fmdNativeRuntime.ready'), 'native boot: ' + path.name)
            return page

        async def manifest(page):
            return json.loads(await page.locator('body > #fmd-native-runtime').text_content())

        async def idle(page):
            await page.wait_for_function('() => !document.getElementById("fmd-image-picker").disabled')

        async def select(page, files=None):
            await page.locator('#fmd-editor').evaluate('(e)=>{e.focus();e.setSelectionRange(e.value.length,e.value.length);}')
            async with page.expect_file_chooser() as chooser:
                await page.click('#btn-insert-image')
            await (await chooser.value).set_files(files or [{'name': 'same[<>&.jpg', 'mimeType': 'image/jpeg', 'buffer': png}])
            await idle(page)

        async def download(page, button, name):
            async with page.expect_download() as pending:
                await page.click(button)
            result = await pending.value
            path = artifacts / name
            await result.save_as(path)
            check(path.stat().st_size > 0, 'nonempty download: ' + name)
            return path

        page = await open_page(initial)
        check(await page.locator('#fmd-editor').input_value() == SOURCE, 'initial source exact')
        check(await page.evaluate('globalThis.sourceRan === undefined'), 'source script remains inert')
        await select(page)
        imported_source = await page.locator('#fmd-editor').input_value()
        imported = await manifest(page)
        key = imported['images'][1]['destination']
        check(len(imported['images']) == 2, 'picker adds one native resource')
        check(key.startswith('fmd-import/') and key.endswith('.png'), 'contents determine PNG destination, not JPEG filename')
        check(key in imported_source and 'data:image' not in imported_source, 'short resource reference in source')
        check(base64.b64decode(imported['images'][1]['bytes']) == png, 'exact imported bytes in saved manifest')
        await page.wait_for_function('''() => {
          const d=document.querySelector('#fmd-content iframe')?.contentDocument;
          const imgs=d?.querySelectorAll('img'); return imgs?.length===2 && [...imgs].every(i=>i.complete && i.naturalWidth===2);
        }''')
        widths = await page.evaluate('() => [...document.querySelector("#fmd-content iframe").contentDocument.images].map(i=>i.naturalWidth)')
        check(widths == [2, 2], 'both initial and imported images decoded in native preview')

        # Browser undo/redo (not simulated source replacement) must keep assets.
        await page.locator('#fmd-editor').press('Control+z')
        check(await page.locator('#fmd-editor').input_value() == SOURCE, 'native browser undo restores source')
        check(len((await manifest(page))['images']) == 2, 'undo preserves unused resource for redo')
        unused_path = await download(page, '#btn-save-html', 'unused.workspace.html')
        unused = await open_page(unused_path)
        check(await unused.locator('#fmd-editor').input_value() == SOURCE, 'saved undone source reopens exactly')
        check(len((await manifest(unused))['images']) == 2, 'unused resource survives reopening')
        await page.locator('#fmd-editor').press('Control+Shift+z')
        check(await page.locator('#fmd-editor').input_value() == imported_source, 'native browser redo restores reference')

        # Same task: input + export occurs before a debounce callback can run.
        latest = imported_source + '\nPDF must see this newest revision'
        async with page.expect_download() as pending:
            await page.evaluate('''text => { const e=document.getElementById('fmd-editor');e.value=text;
              e.dispatchEvent(new Event('input',{bubbles:true}));document.getElementById('btn-export-pdf').click(); }''', latest)
        pdf_path = artifacts / 'dispatch.pdf-adapter'
        await (await pending.value).save_as(pdf_path)
        receipt = json.loads(pdf_path.read_bytes().split(b'\n', 1)[1])
        check(receipt['source'] == latest, 'PDF receives latest source before debounce')
        check(receipt['images'] == [[x['destination'], x['bytes']] for x in imported['images']], 'PDF receives exact initial and imported images')
        check(receipt['weights'] == [555, 0, 0, 0, 0], 'font pins survive import')
        check(receipt['page'] == [842, 595, 36, 40, 44, 48] and receipt['scale'] == 1.25, 'PDF geometry and typography survive import')
        first_save = await download(page, '#btn-save-html', 'first.workspace.html')
        reopened = await open_page(first_save)
        check(await reopened.locator('#fmd-editor').input_value() == latest, 'saved source with imported reference reopens')
        check((await manifest(reopened))['images'] == imported['images'], 'native resource bytes survive first reopening')
        await select(reopened)
        second = await manifest(reopened)
        check(len(second['images']) == 3, 'second generation accepts further imports')
        check(second['images'][1] == imported['images'][1], 'same filename never overwrites older resource')
        check(second['images'][2]['destination'] != key, 'new insertion receives a distinct identity')
        second_save = await download(reopened, '#btn-save-html', 'second.workspace.html')
        final = await open_page(second_save)
        check((await manifest(final))['images'] == second['images'], 'all generations survive second reopening')
        check(await final.evaluate('globalThis.sourceRan === undefined'), 'saved source remains inert')

        # Real decoder rejection after a good first file: no partial batch.
        before_source = await final.locator('#fmd-editor').input_value()
        before_data = await final.locator('#fmd-native-runtime').text_content()
        await select(final, [{'name':'good.png','mimeType':'image/png','buffer':png},
                             {'name':'fake.png','mimeType':'image/png','buffer':b'<svg onload="alert(1)"/>'}])
        check(await final.locator('#fmd-editor').input_value() == before_source, 'failed batch preserves source')
        check(await final.locator('#fmd-native-runtime').text_content() == before_data, 'failed batch preserves serialized resources')
        check('failed' in await final.locator('#fmd-save-status').text_content(), 'failed batch reports error')

        # Exercise real publisher rollback after an injected editing failure.
        await final.evaluate('''() => {
          globalThis.savedCommand=document.execCommand;document.execCommand=()=>false;
          const e=document.getElementById('fmd-editor');e.setRangeText=()=>{throw Error('injected editing failure');};
        }''')
        await select(final)
        check(await final.locator('#fmd-editor').input_value() == before_source, 'editing failure preserves source')
        check(await final.locator('#fmd-native-runtime').text_content() == before_data, 'editing failure rolls back serialized resource publication')
        count = await final.evaluate('''() => JSON.parse(new TextDecoder().decode(window.__fmdNativeRuntime.pdf('x')).split(String.fromCharCode(10))[1]).images.length''')
        check(count == 3, 'editing failure also rolls back packed native resources')
        await final.evaluate('''() => {document.execCommand=globalThis.savedCommand;delete document.getElementById('fmd-editor').setRangeText;}''')
        await select(final)
        check(len((await manifest(final))['images']) == 4, 'same image selection retries successfully after failure')

        # Clipboard data is synthetic, not an ambient OS clipboard read. File
        # drops use Chromium's trusted drag input path with a retained local PNG.
        async def paste_file():
            return await final.evaluate('''png => {
              const editor=document.getElementById('fmd-editor');editor.focus();editor.setSelectionRange(0,0);
              const data=new DataTransfer(), bytes=Uint8Array.from(atob(png),c=>c.charCodeAt(0));
              data.items.add(new File([bytes],'clipboard[<>&.jpg',{type:'image/jpeg'}));
              const event=new ClipboardEvent('paste',{clipboardData:data,bubbles:true,cancelable:true});
              editor.dispatchEvent(event);return {prevented:event.defaultPrevented};
            }''', base64.b64encode(png).decode())

        async def drop_file():
            dropped_path = artifacts / 'dropped.png'
            dropped_path.write_bytes(png)
            await final.locator('#fmd-editor').scroll_into_view_if_needed()
            box = await final.locator('#fmd-editor').bounding_box()
            await final.evaluate('''() => {
              globalThis.dropReceipts=[];const editor=document.getElementById('fmd-editor');
              for(const type of ['dragover','drop']) editor.addEventListener(type,event=>{
                dropReceipts.push({type,trusted:event.isTrusted,prevented:event.defaultPrevented,
                  effect:event.dataTransfer.dropEffect});
              });
            }''')
            session = await context.new_cdp_session(final)
            # The protocol delivers Files in protected mode during dragover and
            # actual file bytes on drop, unlike a script-created DataTransfer.
            event = {'x': box['x'] + 30, 'y': box['y'] + 30,
                     'data': {'items': [], 'files': [str(dropped_path)], 'dragOperationsMask': 1}}
            try:
                for kind in ['dragEnter', 'dragOver']:
                    await session.send('Input.dispatchDragEvent', {'type': kind, **event})
                await final.locator('#fmd-editor').evaluate('(e)=>{e.focus();e.setSelectionRange(e.value.length,e.value.length);}')
                await session.send('Input.dispatchDragEvent', {'type': 'drop', **event})
                return await final.evaluate('dropReceipts')
            finally:
                await session.detach()

        pasted = await paste_file()
        await idle(final)
        after_paste = await manifest(final)
        check(pasted['prevented'], 'image paste is handled without default image/HTML insertion')
        check(len(after_paste['images']) == 5, 'image paste publishes a native image resource')
        paste_key = after_paste['images'][-1]['destination']
        check((await final.locator('#fmd-editor').input_value()).startswith('![clipboard'), 'image paste honors current start selection')
        check(paste_key.endswith('.png') and base64.b64decode(after_paste['images'][-1]['bytes']) == png,
              'pasted bytes and MIME admission are exact despite claimed JPEG type')
        dropped = await drop_file()
        await idle(final)
        after_drop = await manifest(final)
        check({event['type'] for event in dropped} == {'dragover', 'drop'} and all(event['prevented'] and event['trusted'] and event['effect'] == 'copy' for event in dropped), 'trusted Chromium file drag/drop is accepted without file navigation')
        check(len(after_drop['images']) == 6, 'file drop uses the same native resource pipeline')
        drop_key = after_drop['images'][-1]['destination']
        check((await final.locator('#fmd-editor').input_value()).endswith('](' + drop_key + ')'), 'file drop honors current end selection')
        await final.wait_for_function('''() => {
          const images=document.querySelector('#fmd-content iframe')?.contentDocument?.images;
          return images?.length===6 && [...images].every(image=>image.complete && image.naturalWidth===2);
        }''')
        widths = await final.evaluate('() => [...document.querySelector("#fmd-content iframe").contentDocument.images].map(i=>i.naturalWidth)')
        check(widths == [2] * 6, 'pasted and dropped images actually decode in the preview')
        unchanged = await final.locator('#fmd-editor').input_value()
        untouched = await final.evaluate('''() => {
          const editor=document.getElementById('fmd-editor'), data=new DataTransfer();
          data.setData('text/plain','ordinary text');data.setData('text/html','<b>ordinary HTML</b>');
          data.setData('text/uri-list','https://not-requested.invalid/image.png');
          const events=[new ClipboardEvent('paste',{clipboardData:data,bubbles:true,cancelable:true}),
            new DragEvent('dragover',{dataTransfer:data,bubbles:true,cancelable:true}),
            new DragEvent('drop',{dataTransfer:data,bubbles:true,cancelable:true})];
          for(const event of events) editor.dispatchEvent(event);
          return events.map(event=>event.defaultPrevented);
        }''')
        check(untouched == [False, False, False], 'text/HTML paste and URL drags remain browser-owned')
        check(await final.locator('#fmd-editor').input_value() == unchanged, 'synthetic non-file events cause no importer source rewrite')
        check((await manifest(final))['images'] == after_drop['images'], 'non-file events do not mutate native resources')
        gestures_save = await download(final, '#btn-save-html', 'paste-drop.workspace.html')
        gestures = await open_page(gestures_save)
        check((await manifest(gestures))['images'] == after_drop['images'], 'pasted and dropped resources survive another save/reopen')
        check(await gestures.locator('#fmd-editor').input_value() == unchanged, 'pasted and dropped references survive another save/reopen')
        receipt = await gestures.evaluate('''() => JSON.parse(new TextDecoder().decode(
          window.__fmdNativeRuntime.pdf(document.getElementById('fmd-editor').value)).split(String.fromCharCode(10))[1])''')
        check(receipt['images'] == [[image['destination'], image['bytes']] for image in after_drop['images']],
              'reopened PDF dispatch includes picker, clipboard and dropped resources together')
        check(not requests, 'zero HTTP(S) requests')
        check(not errors, 'zero uncaught browser errors')
        version = browser.version
        await browser.close()
    report = {'proof': 'Chromium lifecycle with ABI adapters and empty real WASM', 'mode': args.mode,
              'browser': version, 'passed': len(findings), 'assertions': findings, 'requests': requests, 'errors': errors}
    (artifacts / 'report.json').write_text(json.dumps(report, indent=2))
    print(json.dumps({'passed': len(findings), 'mode': args.mode, 'artifacts': str(artifacts), 'browser': version}))


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--mode', choices=['file', 'content'], default='file')
    parser.add_argument('--chromium', default=shutil.which('chromium'))
    asyncio.run(run(parser.parse_args()))
