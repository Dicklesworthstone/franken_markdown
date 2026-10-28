// Host-only browser verification; no browser dependency enters the renderer.
// FMD_PLAYWRIGHT_MODULE=/path/to/playwright/index.mjs FMD_CHROMIUM_BIN=/path/to/chromium
// node tests/support/interactive_presentation_browser_check.mjs generated.html
// The Rust companion test supplies the configured fixture on stdin.
import assert from 'node:assert/strict';
import {readFile} from 'node:fs/promises';
import {pathToFileURL} from 'node:url';
import {resolve} from 'node:path';

const {chromium} = await import(process.env.FMD_PLAYWRIGHT_MODULE
  ? pathToFileURL(resolve(process.env.FMD_PLAYWRIGHT_MODULE)).href : 'playwright');
let html = '';
if (process.argv[2]) html = await readFile(process.argv[2], 'utf8');
else for await (const chunk of process.stdin) html += chunk;
const browser = await chromium.launch({headless: true,
  executablePath: process.env.FMD_CHROMIUM_BIN || undefined,
  args: ['--no-sandbox', '--disable-dev-shm-usage']});
const context = await browser.newContext({acceptDownloads: true, colorScheme: 'light',
  viewport: {width: 1600, height: 900}});
const requests = [], errors = [];
await context.route('**/*', route => {requests.push(route.request().url()); return route.abort();});
const selector = '#fmd-content > iframe[data-fmd-styled-preview]';
const source = page => page.locator('textarea#fmd-editor').inputValue();
const frame = page => page.frameLocator(selector);
const contents = page => frame(page).locator('main.fmd').innerHTML();
async function load(text) {
  const page = await context.newPage();
  page.on('pageerror', error => errors.push(String(error)));
  // Use a real embedding URL: about:blank/setContent hides broken srcdoc
  // fragment links that accidentally navigate to the surrounding workspace.
  await page.route('https://workspace.invalid/**', route => {
    if (route.request().isNavigationRequest() && route.request().frame() === page.mainFrame())
      return route.fulfill({status: 200, contentType: 'text/html; charset=utf-8', body: text});
    requests.push(route.request().url()); return route.abort();
  });
  await page.goto('https://workspace.invalid/notes.html', {waitUntil: 'load'});
  await frame(page).locator('main.fmd').waitFor();
  await page.locator(selector).evaluate(async element => {await element.contentDocument.fonts.ready;});
  return page;
}
async function edit(page, text) {
  // Beforeprint forces the real controller's pending render, as does Save HTML.
  await page.evaluate(text => {
    const editor = document.querySelector('textarea#fmd-editor');
    editor.value = text; editor.dispatchEvent(new Event('input'));
    window.dispatchEvent(new Event('beforeprint'));
  }, text);
  await frame(page).locator('main.fmd').waitFor();
  await page.locator(selector).evaluate(async element => {await element.contentDocument.fonts.ready;});
}
async function download(page, button) {
  const pending = page.waitForEvent('download');
  await page.locator(button).click();
  const result = await pending;
  assert.equal(await result.failure(), null);
  return readFile(await result.path(), 'utf8');
}
const manifest = (page, id) => page.locator('body > script#' + id).evaluate(element => JSON.parse(element.textContent));
try {
  const page = await load(html), original = await source(page), initial = await contents(page);
  const presentation = await manifest(page, 'fmd-presentation'), images = await manifest(page, 'fmd-image-assets');
  assert.equal(presentation.toc, true); assert.equal(presentation.tocDepth, 2);
  assert.ok(images.length >= 2, 'fixture retains a used and unused asset');
  assert.equal(await page.locator('textarea#fmd-editor').isVisible(), true, 'document CSS reached editor');
  assert.equal(await frame(page).locator('html').getAttribute('lang'), 'fr');
  assert.match(await frame(page).locator('body').evaluate(element => getComputedStyle(element).fontFamily), /FMD Body/);
  assert.equal(await frame(page).locator('body').evaluate(element => getComputedStyle(element).fontSize), '20px');
  assert.equal(await frame(page).locator('body').evaluate(element => getComputedStyle(element).color), 'rgb(70, 50, 30)');
  assert.equal(await page.locator(selector).evaluate(element => element.contentDocument.fonts.size), 5);

  const edited = '# fmd-editor\n\n## Revised é Ω Ж\n\n### Hidden from navigation\n\n'
    + '**Bold Ω** and *Italic Ж*, ***both é*** and ' + String.fromCharCode(96) + 'code Ω' + String.fromCharCode(96)
    + '\n\n![Kept](chart.svg)\n\n![Previously unused](unused.svg)\n\n'
    + '<script>globalThis.documentExecuted=true</script>\n\n'
    + '![Unadmitted external image](https://example.invalid/no-fetch.png)\n\n'
    + 'A footnote[^note].\n\n[^note]: A retained note.\n';
  await edit(page, edited);
  assert.equal(await frame(page).locator('nav.toc a').count(), 2);
  assert.deepEqual(await frame(page).locator('nav.toc a').allTextContents(), ['fmd-editor', 'Revised é Ω Ж']);
  await frame(page).locator('nav.toc a').nth(1).click();
  await frame(page).locator('sup a[href^="#fn-"]').click();
  assert.equal(await page.locator(selector).evaluate(element => element.contentDocument.URL), 'about:srcdoc');
  assert.equal(await source(page), edited);
  assert.equal(await frame(page).locator('img[src^="data:image/"]').count(), 2);
  assert.equal(await frame(page).locator('img[src^="data:image/"]').evaluateAll(images => images.every(image => image.complete && image.naturalWidth)), true);
  assert.equal(await page.locator('textarea#fmd-editor').isVisible(), true);
  assert.equal(await page.evaluate(() => globalThis.documentExecuted), undefined);
  assert.equal(await page.locator(selector).evaluate(element => element.contentWindow.documentExecuted), undefined);
  assert.equal(await page.locator(selector).evaluate(async element => {
    const fonts = element.contentDocument.fonts;
    const regular = await fonts.load('20px "FMD Body"', 'éΩЖ');
    const bold = await fonts.load('bold 20px "FMD Body"', 'Ω');
    const italic = await fonts.load('italic 20px "FMD Body"', 'Ж');
    return [regular, bold, italic].every(faces => faces.length > 0 && faces.every(face => face.status === 'loaded'));
  }), true, 'later glyphs/styles lost their complete font faces');
  assert.equal(await download(page, '#btn-save-markdown'), edited);

  await page.locator('#btn-zoom-in').click();
  await page.locator('#btn-theme-toggle').click();
  await page.waitForFunction(() => document.querySelector('#fmd-content iframe')?.contentDocument?.body?.style.zoom === '1.1');
  const view = await page.locator(selector).evaluate(element => element.getAttribute('data-fmd-display'));
  const saved = await download(page, '#btn-save-html'), reopened = await load(saved);
  assert.equal(await source(reopened), edited);
  assert.deepEqual(await manifest(reopened, 'fmd-presentation'), presentation);
  assert.deepEqual(await manifest(reopened, 'fmd-image-assets'), images);
  assert.equal(await reopened.locator(selector).getAttribute('data-fmd-display'), view);
  assert.equal(await frame(reopened).locator('nav.toc a').count(), 2);
  await frame(reopened).locator('nav.toc a').nth(1).click();
  assert.equal(await reopened.locator(selector).evaluate(element => element.contentDocument.URL), 'about:srcdoc');
  assert.equal(await reopened.locator('textarea#fmd-editor').isVisible(), true);

  // Removing every image does not remove the resources needed by later edits.
  await edit(reopened, 'No headings or images now.');
  assert.equal(await frame(reopened).locator('nav.toc').count(), 0);
  const empty = await load(await download(reopened, '#btn-save-html'));
  assert.deepEqual(await manifest(empty, 'fmd-image-assets'), images);
  await edit(empty, edited);
  assert.equal(await frame(empty).locator('img[src^="data:image/"]').count(), 2);
  assert.equal(await frame(empty).locator('nav.toc a').count(), 2);

  await edit(page, original);
  assert.equal(await contents(page), initial, 'exact undo lost native content');
  assert.equal(await page.locator(selector).getAttribute('data-fmd-display'), view, 'exact undo reset viewing settings');

  // Instrument the browser's print entry point, not the renderer. Verify the
  // real click waits for its newly created frame and prints the latest edit.
  await page.evaluate(() => {
    const preview = document.getElementById('fmd-content');
    new MutationObserver(() => {
      const frame = preview.querySelector('iframe');
      frame?.addEventListener('load', () => {
        const fonts = frame.contentDocument.fonts;
        Object.defineProperty(fonts, 'ready', {value: Promise.all([
          fonts.ready, new Promise(resolve => setTimeout(resolve, 350)),
        ])});
        frame.contentWindow.print = () => {
          window.printedDocument = frame.contentDocument.documentElement.outerHTML;
          window.printedAt = performance.now();
        };
      }, {once: true, capture: true});
    }).observe(preview, {childList: true});
  });
  const latest = edited.replace('Revised é Ω Ж', 'Printed immediately Ω');
  await page.evaluate(text => {
    const editor = document.querySelector('textarea#fmd-editor');
    editor.value = text; editor.dispatchEvent(new Event('input'));
    window.printRequestedAt = performance.now();
    document.getElementById('btn-export-pdf').click();
  }, latest);
  await page.waitForFunction(() => typeof window.printedDocument === 'string');
  const printed = await page.evaluate(() => window.printedDocument);
  assert.ok(await page.evaluate(() => window.printedAt - window.printRequestedAt >= 300),
    'print did not await fonts beyond the pending preview debounce');
  assert.match(printed, /Printed immediately Ω/);
  assert.match(printed, /data:image\/svg\+xml;base64,/);
  assert.match(printed, /font-family: "FMD Body"/);
  assert.match(printed, /class="toc"/);
  assert.deepEqual(requests, [], 'configured document fetched an external resource');
  assert.deepEqual(errors, [], 'workspace raised browser errors');
  console.log(JSON.stringify({verified: ['CSS isolation', 'full fonts', 'TOC after edits', 'explicit images',
    'save/reopen', 'unused resources', 'exact undo', 'zoom/theme persistence', 'latest-revision print'],
    networkRequests: requests.length, runtimeErrors: errors.length}));
} finally {
  await context.close(); await browser.close();
}
