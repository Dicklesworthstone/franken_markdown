"""Real-browser image authoring checks over a generated interactive workspace.

Usage: python tests/support/interactive_import_browser_check.py document.html
Needs Playwright/Chromium on the verification host only. Fixtures are real PNG,
progressive JPEG and GIF bytes; imports use the actual file input.
"""
import argparse
import base64
import json
from pathlib import Path
import shutil
import struct
import zlib
from playwright.sync_api import sync_playwright

parser = argparse.ArgumentParser()
parser.add_argument("html", type=Path)
parser.add_argument("--chromium", default=shutil.which("chromium"))
args = parser.parse_args()
errors, requests = [], []
checks = 0

def check(ok, message):
    global checks
    assert ok, f"{message}; browser errors: {errors}"
    checks += 1

def chunk(tag, data):
    return struct.pack('>I', len(data)) + tag + data + struct.pack('>I', zlib.crc32(tag + data))

def png(width=2, height=1):
    # Dimensions can be intentionally wrong for pre-decode refusal fixtures.
    return b'\x89PNG\r\n\x1a\n' + chunk(b'IHDR', struct.pack('>IIBBBBB', width, height, 8, 6, 0, 0, 0)) + chunk(b'IDAT', zlib.compress(b'\0\xff\0\0\xff\0\xff\0\xff')) + chunk(b'IEND', b'')

JPEG = base64.b64decode('/9j/4AAQSkZJRgABAQAAAQABAAD/2wBDAAgGBgcGBQgHBwcJCQgKDBQNDAsLDBkSEw8UHRofHh0aHBwgJC4nICIsIxwcKDcpLDAxNDQ0Hyc5PTgyPC4zNDL/2wBDAQkJCQwLDBgNDRgyIRwhMjIyMjIyMjIyMjIyMjIyMjIyMjIyMjIyMjIyMjIyMjIyMjIyMjIyMjIyMjIyMjIyMjL/wgARCAACAAMDASIAAhEBAxEB/8QAFQABAQAAAAAAAAAAAAAAAAAAAAX/xAAUAQEAAAAAAAAAAAAAAAAAAAAF/9oADAMBAAIQAxAAAAGOFD//xAAUEAEAAAAAAAAAAAAAAAAAAAAA/9oACAEBAAEFAn//xAAUEQEAAAAAAAAAAAAAAAAAAAAA/9oACAEDAQE/AX//xAAUEQEAAAAAAAAAAAAAAAAAAAAA/9oACAECAQE/AX//xAAUEAEAAAAAAAAAAAAAAAAAAAAA/9oACAEBAAY/An//xAAUEAEAAAAAAAAAAAAAAAAAAAAA/9oACAEBAAE/IX//2gAMAwEAAgADAAAAEAP/xAAUEQEAAAAAAAAAAAAAAAAAAAAA/9oACAEDAQE/EH//xAAUEQEAAAAAAAAAAAAAAAAAAAAA/9oACAECAQE/EH//xAAUEAEAAAAAAAAAAAAAAAAAAAAA/9oACAEBAAE/EH//2Q==')
GIFS = [base64.b64decode((Path(__file__).resolve().parents[1] / 'fixtures/gif' / (name + '.base64')).read_text())
        for name in ['animated-colors', 'interlaced-palette']]

def upload(name, data, mime='application/octet-stream'):
    return {'name': name, 'mimeType': mime, 'buffer': data}

with sync_playwright() as p:
    browser = p.chromium.launch(executable_path=args.chromium, headless=True, args=['--no-sandbox'])
    context = browser.new_context(accept_downloads=True, viewport={'width':1800,'height':1000})
    context.set_default_timeout(5000)
    context.route('**/*', lambda route: (requests.append(route.request.url), route.abort()))
    template = args.html.read_text(encoding='utf-8')
    editor = 'body > #fmd-app-body > #editor-pane > textarea#fmd-editor'
    picker = 'body > .fmd-app-header #fmd-image-picker'
    status = '#editor-pane > .fmd-pane-header > #fmd-save-status'

    def load(html=template):
        page = context.new_page()
        page.on('pageerror', lambda error: errors.append(str(error)))
        page.set_content(html, wait_until='load')
        return page

    def text(page):
        return page.locator(editor).input_value()

    def edit(page, source, start=None, end=None):
        page.locator(editor).fill(source)
        page.locator(editor).evaluate('(el, range) => el.setSelectionRange(range[0],range[1])', [start if start is not None else len(source), end if end is not None else len(source)])

    def insert(page, files, chooser=False):
        if chooser:
            with page.expect_file_chooser() as pending:
                page.locator('body > .fmd-app-header #btn-insert-image').click()
            pending.value.set_files(files)
        else:
            page.locator(picker).set_input_files(files)
        page.wait_for_function("!document.querySelector('body > .fmd-app-header #btn-insert-image').disabled")
        return page.locator(status).text_content()

    def delay_image_reads(page):
        # Real FileReader bytes complete; only delivery to the importer is held.
        # Capturing the callback deliberately exercises queued completion after
        # cancellation has cleared the reader's current event handlers.
        page.evaluate("""() => {
          window.delayedImageReads = [];
          const read = FileReader.prototype.readAsArrayBuffer;
          FileReader.prototype.readAsArrayBuffer = function(file) {
            const loaded = this.onload;
            this.onload = event => delayedImageReads.push(() => loaded?.call(this, event));
            return read.call(this, file);
          };
        }""")

    def save(page, button):
        with page.expect_download() as pending:
            page.locator('body > .fmd-app-header #' + button).click()
        result = pending.value
        check(result.failure() is None, 'download failed')
        return Path(result.path()).read_bytes()

    page = load()
    edit(page, 'BEFORE selected AFTER', 7, 15)
    # Real chooser, misleading extension/type, and literal Markdown metacharacters.
    name = '[evil]_*`&amp;<x>.jpg'
    message = insert(page, [upload(name,png(),'text/html')], chooser=True)
    check(message.startswith('Inserted 1 image'), 'valid PNG import failed: ' + message)
    inserted = text(page)
    check(inserted.startswith('BEFORE \n\n![') and inserted.endswith('\n\n AFTER'), 'insertion did not replace only the selection')
    check('data:image/png;base64,' + base64.b64encode(png()).decode() in inserted, 'file MIME/name changed byte-determined payload')
    page.evaluate("window.dispatchEvent(new Event('beforeprint'))")
    check(page.locator('main#preview-pane img').count() == 1, 'image absent after synchronous preview refresh')
    check(page.locator('main#preview-pane img').get_attribute('alt') == name, 'filename became markup instead of literal alt text')
    page.wait_for_function('document.querySelector("main#preview-pane img").naturalWidth === 2')
    check(True, 'imported image decodes')
    check(save(page, 'btn-save-markdown') == inserted.encode(), 'Markdown export is not portable byte-exact source')
    workspace = save(page, 'btn-save-html')
    reopened = load(workspace.decode())
    check(text(reopened) == inserted, 'saved workspace lost imported source')
    check(reopened.locator('#btn-cancel-image-import').count() == 1 and
          reopened.locator('#btn-cancel-image-import').is_hidden(), 'reopened workspace retained a busy or duplicate cancellation control')
    edit(reopened, inserted + '\n\nEdited after reopening')
    reopened.evaluate("window.dispatchEvent(new Event('beforeprint'))")
    check(reopened.locator('main#preview-pane img').get_attribute('src').endswith(base64.b64encode(png()).decode()), 'image lost after reopening and editing')
    # Browser undo should remove the single insertion, preserving prior history.
    page.locator(editor).focus()
    page.keyboard.press('Control+z')
    check(text(page) == 'BEFORE selected AFTER', 'native undo did not undo the insertion as one edit')
    page.keyboard.press('Control+Shift+z')
    check(text(page) == inserted, 'native redo did not restore the insertion')

    page = load()
    edit(page, 'Batch')
    message = insert(page, [upload('first.png',png()),upload('second.png',JPEG)])
    check(message.startswith('Inserted 2 images'), 'batch failed: ' + message)
    check(text(page).index('![first.png]') < text(page).index('![second.png]'), 'batch reordered file selection')
    check('data:image/jpeg;base64,' in text(page), 'progressive JPEG not admitted by content')
    page.evaluate("window.dispatchEvent(new Event('beforeprint'))")
    page.wait_for_function('Array.from(document.querySelectorAll("main#preview-pane img")).every(img => img.complete && img.naturalWidth > 0)')
    check(page.locator('main#preview-pane img').count() == 2, 'batch preview lost an image')

    page = load()
    edit(page, 'GIF images')
    check('image/gif' in page.locator(picker).get_attribute('accept'), 'file chooser excludes supported GIF images')
    message = insert(page, [upload('animated.png', GIFS[0], 'image/png'), upload('interlaced.gif', GIFS[1])], chooser=True)
    check(message.startswith('Inserted 2 images'), 'GIF batch failed: ' + message)
    for encoded in GIFS:
        check('data:image/gif;base64,' + base64.b64encode(encoded).decode() in text(page), 'GIF import changed original frame bytes')
    page.evaluate("window.dispatchEvent(new Event('beforeprint'))")
    page.wait_for_function('''() => {
      const images = document.querySelectorAll('main#preview-pane img');
      return images.length === 2 && images[0].naturalWidth === 2 && images[1].naturalWidth === 96;
    }''')
    check(True, 'animated and interlaced GIFs decode in the real preview')
    original = text(page)
    check(save(page, 'btn-save-markdown') == original.encode(), 'GIF Markdown download loses original animation bytes')
    reopened_gifs = load(save(page, 'btn-save-html').decode())
    check(text(reopened_gifs) == original, 'GIF source changes through save/reopen')

    page = load()
    edit(page, 'Keep unchanged')
    for files in [
        [upload('valid.png',png()),upload('fake.png',b'<svg onload="alert(1)"/>','image/png')],
        [upload('huge.png',png(5000,5000))],
        [upload('too-many.png',png())] * 9,
        [upload('empty.png',b'')],
        [upload('truncated.gif',GIFS[0][:-1])],
        # A plausible frame passes metadata but must still fail real decoding.
        [upload('bad.jpg',bytes([255,216,255,192,0,11,8,0,1,0,1,1,1,17,0,255,217]))],
    ]:
        message = insert(page, files)
        check(message.startswith('Image insertion failed:') and text(page) == 'Keep unchanged', 'failed batch mutated source: ' + message)
    message = insert(page, [upload('retry.png',png())])
    check(message.startswith('Inserted 1 image'), 'failed import prevented retry')
    message = insert(page, [upload('retry.png',png())])
    check(message.startswith('Inserted 1 image') and text(page).count('![retry.png]') == 2, 'same file could not be reselected')

    # Delay actual file read delivery, then change and undo source: revision, not
    # string equality alone, must reject stale asynchronous insertion.
    page = load()
    edit(page, 'Original')
    delay_image_reads(page)
    page.locator(picker).set_input_files([upload('slow.png',png())])
    page.wait_for_function('delayedImageReads.length === 1')
    check(page.locator('body > .fmd-app-header #btn-insert-image').is_disabled(), 'parallel import not disabled')
    edit(page, 'New work')
    check('document changed' in page.locator(status).text_content() and
          not page.locator('#btn-insert-image').is_disabled(), 'source editing did not cancel and release the importer immediately')
    edit(page, 'Original')
    current_status = page.locator(status).text_content()
    page.evaluate('delayedImageReads.shift()()')
    page.wait_for_function("!document.querySelector('body > .fmd-app-header #btn-insert-image').disabled")
    check(text(page) == 'Original' and page.locator(status).text_content() == current_status, 'stale import overwrote intervening edits/undo or their current status')

    # A same-task synthetic composition event reaches a genuinely active
    # FileReader, proving physical abort without delaying any native callback.
    page = load()
    edit(page, 'Original')
    aborts = page.evaluate("""encoded => {
      let aborts = 0;
      const abort = FileReader.prototype.abort;
      FileReader.prototype.abort = function() { aborts++; return abort.call(this); };
      const files = new DataTransfer();
      files.items.add(new File([Uint8Array.from(atob(encoded), c => c.charCodeAt(0))], 'active.png'));
      const picker = document.querySelector('#fmd-image-picker');
      picker.files = files.files; picker.dispatchEvent(new Event('change'));
      document.querySelector('#fmd-editor').dispatchEvent(new CompositionEvent('compositionstart'));
      return aborts;
    }""", base64.b64encode(png()).decode())
    check(aborts == 1 and text(page) == 'Original', 'composition did not physically abort the active FileReader')
    page.close()

    # These are synthetic lifecycle/composition events, not OS IME gestures.
    # File selection, FileReader bytes, decoding and editor insertion are real.
    for transition in ['composition', 'readOnly', 'disabled', 'suspension', 'cancel']:
        page = load()
        edit(page, 'Original')
        delay_image_reads(page)
        page.locator(picker).set_input_files([upload('abandoned.png', png())])
        page.wait_for_function('delayedImageReads.length === 1')
        if transition == 'composition':
            page.locator(editor).dispatch_event('compositionstart')
        elif transition in ['readOnly', 'disabled']:
            page.locator(editor).evaluate('(e, key) => { e[key] = true; }', transition)
        elif transition == 'suspension':
            page.evaluate("window.dispatchEvent(new Event('pagehide'))")
        else:
            page.locator('#btn-cancel-image-import').click()
        check(text(page) == 'Original' and 'cancelled' in page.locator(status).text_content(), transition + ' did not cancel the pending import')
        if transition == 'composition':
            page.locator(editor).dispatch_event('compositionend')
        elif transition in ['readOnly', 'disabled']:
            page.locator(editor).evaluate('(e, key) => { e[key] = false; }', transition)
        elif transition == 'suspension':
            page.evaluate("window.dispatchEvent(new Event('pageshow'))")
        page.wait_for_function("!document.querySelector('#btn-insert-image').disabled")
        page.locator(picker).set_input_files([upload('fresh.png', png())])
        page.wait_for_function('delayedImageReads.length === 2')
        page.evaluate('delayedImageReads.shift()()')
        check(text(page) == 'Original' and page.locator('#btn-insert-image').is_disabled() and
              page.locator(status).text_content() == 'Reading selected images…', transition + ' late completion interfered with the retry')
        page.evaluate('delayedImageReads.shift()()')
        page.wait_for_function("!document.querySelector('#btn-insert-image').disabled")
        check('![fresh.png]' in text(page) and 'abandoned.png' not in text(page), transition + ' retry inserted stale source')
        page.close()

    page = load()
    edit(page, 'Original')
    delay_image_reads(page)
    page.locator(picker).set_input_files([upload('timed-out.png', png())])
    page.wait_for_function('delayedImageReads.length === 1')
    page.wait_for_function("!document.querySelector('#btn-insert-image').disabled", timeout=12000)
    check(text(page) == 'Original' and 'ten seconds' in page.locator(status).text_content(), 'real read deadline did not preserve source and release controls')
    page.evaluate('delayedImageReads.shift()()')
    check(text(page) == 'Original', 'timed-out read later inserted source')
    page.close()

    # Saved read mode must switch through the controller, keeping its private
    # mode state and next toggle coherent. Also exercise the fallback inserter.
    page = load()
    edit(page, 'Read mode')
    page.locator('body > .fmd-app-header #btn-toggle-view').click()
    page.evaluate('document.execCommand = undefined')
    message = insert(page,[upload('fallback.png',png())],chooser=True)
    check(message.startswith('Inserted 1 image') and 'data:image/png' in text(page), 'fallback insertion failed')
    check(page.locator('body > #fmd-app-body').evaluate('e => e.classList.contains("view-split")'), 'import did not enter edit mode')
    page.locator('body > .fmd-app-header #btn-toggle-view').click()
    check(page.locator('body > #fmd-app-body').evaluate('e => e.classList.contains("view-read")'), 'controller view state diverged')
    check(not requests, f'local imports made network requests: {requests}')
    check(not errors, 'browser runtime errors')
    browser.close()
print(json.dumps({'checks_passed':checks,'network_requests':requests,'runtime_errors':errors}))
