"""Real-browser image authoring checks over a generated interactive workspace.

Usage: python tests/support/interactive_import_browser_check.py document.html
Needs Playwright/Chromium on the verification host only. Fixtures are real PNG
and progressive JPEG bytes; imports are driven through the actual file input.
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
    edit(page, 'Keep unchanged')
    for files in [
        [upload('valid.png',png()),upload('fake.png',b'<svg onload="alert(1)"/>','image/png')],
        [upload('huge.png',png(5000,5000))],
        [upload('too-many.png',png())] * 9,
        [upload('empty.png',b'')],
        # A plausible frame passes metadata but must still fail real decoding.
        [upload('bad.jpg',bytes([255,216,255,192,0,11,8,0,1,0,1,1,1,17,0,255,217]))],
    ]:
        message = insert(page, files)
        check(message.startswith('Image insertion failed:') and text(page) == 'Keep unchanged', 'failed batch mutated source: ' + message)
    message = insert(page, [upload('retry.png',png())])
    check(message.startswith('Inserted 1 image'), 'failed import prevented retry')
    message = insert(page, [upload('retry.png',png())])
    check(message.startswith('Inserted 1 image') and text(page).count('![retry.png]') == 2, 'same file could not be reselected')

    # Delay actual file reading, then change and undo source: revision, not
    # string equality alone, must reject stale asynchronous insertion.
    page = load()
    edit(page, 'Original')
    page.evaluate("""() => {
      const read = File.prototype.arrayBuffer;
      File.prototype.arrayBuffer = function() {
        return new Promise(resolve => { window.finishImageRead = () => read.call(this).then(resolve); });
      };
    }""")
    page.locator(picker).set_input_files([upload('slow.png',png())])
    page.wait_for_function('typeof window.finishImageRead === "function"')
    check(page.locator('body > .fmd-app-header #btn-insert-image').is_disabled(), 'parallel import not disabled')
    edit(page, 'New work')
    edit(page, 'Original')
    page.evaluate('window.finishImageRead()')
    page.wait_for_function("!document.querySelector('body > .fmd-app-header #btn-insert-image').disabled")
    check(text(page) == 'Original' and 'document changed' in page.locator(status).text_content(), 'stale import overwrote intervening edits/undo')

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
