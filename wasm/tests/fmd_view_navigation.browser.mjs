// Real Chromium URL resolution, redirects, shadow DOM, scrolling and keyboard
// activation over the production component. Only the imported Markdown renderer
// is doubled: these fixtures do not claim native parsing, WASM or font parity.
// A loopback fixture server supplies real HTTP redirects; all external requests
// are blocked. The component's source-fetch and DOM code run unchanged.
import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { readFile } from "node:fs/promises";
import { createServer } from "node:http";
import { resolve } from "node:path";
import { pathToFileURL } from "node:url";

const component = await readFile(new URL("../fmd-view.js", import.meta.url), "utf8");
const { chromium } = await import(process.env.FMD_PLAYWRIGHT_MODULE
  ? pathToFileURL(resolve(process.env.FMD_PLAYWRIGHT_MODULE)).href : "playwright");
const browser = await chromium.launch({ headless: true,
  ...(process.env.FMD_CHROMIUM ? { executablePath: process.env.FMD_CHROMIUM } : {}),
  args: ["--no-sandbox", "--disable-dev-shm-usage"] });
const png = "iVBORw0KGgoAAAANSUhEUgAAAAIAAAABCAYAAAD0In+KAAAADklEQVR4nGP4z8DwHwQBEPgD/U6VwW8AAAAASUVORK5CYII=";
const server = createServer((request, response) => {
  const url = new URL(request.url, origin);
  const send = (type, body, status = 200, headers = {}) => {
    response.writeHead(status, { "content-type": type, ...headers }); response.end(body);
  };
  if (url.pathname === "/host/index.html") return send("text/html",
    '<!doctype html><html><head><title>Component navigation</title></head><body><h2 id="section">Host collision</h2><script type="module" src="/fmd-view.js"></script></body></html>');
  if (url.pathname === "/fmd-view.js") return send("text/javascript", component);
  if (url.pathname === "/franken_markdown.js") return send("text/javascript",
    "export const createRenderer = async () => globalThis.fmdViewFixtureRenderer;");
  if (url.pathname === "/docs/current.md") return send("text/plain", "", 302, { location: sourceUrl });
  if (["/release/guide.md", "/old/source.md", "/new/source.md"].includes(url.pathname))
    return send("text/plain", url.pathname);
  if (url.pathname === "/release/images/chart.png") return send("image/png", Buffer.from(png, "base64"));
  send("text/plain", "Fixture not found", 404);
});
await new Promise((yes, no) => { server.once("error", no); server.listen(0, "127.0.0.1", yes); });
const origin = `http://127.0.0.1:${server.address().port}`, sourceUrl = `${origin}/release/guide.md?revision=1`;
const html = `<main class="fmd">
  <a id="jump" href="#section"><strong>Section</strong></a>
  <a id="relative" href="../intro.md#next">Other document</a>
  <a id="root-relative" href="/license">License</a>
  <a id="query" href="?mode=print#section">Other query</a>
  <a id="absolute-self" href="${sourceUrl}#section">Same document</a>
  <a id="relative-self" href="./guide.md?revision=1#section">Same relative document</a>
  <a id="absolute-external" href="https://outside.test/help#section">External</a>
  <a id="mail" href="mailto:help@example.test">Mail</a>
  <a id="protocol-relative" href="//outside.test/help">Scheme relative</a>
  <a id="blank" href="#section" target="_blank">New context</a>
  <a id="download" href="#section" download>Download</a>
  <a id="missing" href="#absent-target">Missing</a>
  <img id="relative-image" src="images/chart.png" alt="Chart">
  <img id="embedded-image" src="data:image/png;base64,${png}" alt="Embedded">
  <div style="height:1500px"></div>
  <h2 id="section">Section target</h2><a id="after-section" href="#fn-1">Next link</a>
  <sup id="fnref-1"><a id="note" href="#fn-1">1</a></sup>
  <div style="height:700px"></div>
  <ol><li id="fn-1">Note <a id="back" href="#fnref-1">Back</a></li></ol>
  <a id="unicode" href="#caf%C3%A9">Unicode</a><h2 id="café">Café</h2>
  <a id="percent" href="#literal%2520">Literal percent</a><h2 id="literal%20">Percent target</h2>
  <a id="malformed" href="#bad%escape">Malformed escape</a><h2 id="bad%escape">Exact target</h2>
</main>`;
const reports = [];
async function test(name, work) {
  const context = await browser.newContext({ viewport: { width: 1000, height: 700 },
    userAgent: "franken_markdown-agent (+https://github.com/Dicklesworthstone/franken_markdown)" });
  const requests = [], errors = [];
  await context.route("**/*", async route => {
    const url = new URL(route.request().url());
    requests.push(url.href);
    return url.origin === origin ? route.continue() : route.abort();
  });
  const page = await context.newPage();
  page.setDefaultTimeout(5000);
  page.on("pageerror", error => errors.push(String(error)));
  await page.addInitScript(({ html }) => {
    globalThis.fmdViewFixtureRenders = [];
    globalThis.fmdViewFixtureRenderer = { renderHtml(markdown, options) {
      const output = { bytes: new TextEncoder().encode(html), text: () => html,
        sourceLength: new TextEncoder().encode(markdown).length, diagnostics: [] };
      const call = { markdown, options, output };
      globalThis.fmdViewFixtureRenders.push(call);
      if (globalThis.fmdViewFixtureDelay) return new Promise((resolve, reject) => Object.assign(call, { resolve, reject }));
      return output;
    } };
    globalThis.fmdViewFixtureEvents = [];
    globalThis.fmdViewFixtureErrors = [];
  }, { html });
  try {
    await page.goto(`${origin}/host/index.html`);
    await page.evaluate(() => customElements.whenDefined("fmd-view"));
    await work(page, requests);
    assert.deepEqual(errors, [], "no uncaught browser errors");
    assert.deepEqual(await page.evaluate(() => globalThis.fmdViewFixtureErrors), [], "no component errors");
    reports.push({ name, pass: true });
  } catch (error) {
    reports.push({ name, pass: false, error: String(error.stack ?? error), requests,
      browserErrors: errors, componentErrors: await page.evaluate(() => globalThis.fmdViewFixtureErrors).catch(() => null) });
  } finally { await context.close(); }
}
async function mount(page, src = "/docs/current.md", id = "view") {
  await page.evaluate(({ src, id }) => {
    const element = document.createElement("fmd-view");
    element.id = id; element.textContent = "Inline source";
    if (src !== null) element.setAttribute("src", src);
    element.addEventListener("fmd-rendered", event => globalThis.fmdViewFixtureEvents.push({ id, detail: event.detail }));
    element.addEventListener("fmd-error", event => globalThis.fmdViewFixtureErrors.push(event.detail));
    document.body.append(element);
  }, { src, id });
  await page.waitForFunction(id => document.getElementById(id).shadowRoot.querySelector("#jump"), id);
}
async function focused(page, id) {
  return page.evaluate(id => document.getElementById(id).shadowRoot.activeElement?.id ?? null, id);
}
async function preventNavigation(page) {
  await page.evaluate(() => {
    globalThis.fmdViewFixtureClicks = [];
    const element = document.getElementById("view");
    for (const type of ["click", "auxclick"]) element.addEventListener(type, event => {
      globalThis.fmdViewFixtureClicks.push({ type, prevented: event.defaultPrevented, button: event.button });
      // Observe the component's decision before cancelling external navigation.
      event.preventDefault();
    });
  });
}

try {
  await test("fetched links and real image loads follow the final redirected Markdown URL", async (page, requests) => {
    await mount(page);
    const actual = await page.evaluate(() => {
      const root = document.getElementById("view").shadowRoot;
      return Object.fromEntries(["jump", "relative", "root-relative", "query", "protocol-relative", "mail", "absolute-external"]
        .map(id => [id, root.getElementById(id).getAttribute("href")]));
    });
    assert.deepEqual(actual, { jump: `${sourceUrl}#section`, relative: `${origin}/intro.md#next`,
      "root-relative": `${origin}/license`, query: `${origin}/release/guide.md?mode=print#section`,
      "protocol-relative": "http://outside.test/help", mail: "mailto:help@example.test",
      "absolute-external": "https://outside.test/help#section" });
    await page.waitForFunction(() => document.getElementById("view").shadowRoot.getElementById("relative-image").naturalWidth === 2);
    assert.ok(requests.includes(`${origin}/release/images/chart.png`));
    assert.ok(!requests.includes(`${origin}/host/images/chart.png`));
    assert.equal(await page.evaluate(() => document.getElementById("view").shadowRoot.getElementById("embedded-image").getAttribute("src")), `data:image/png;base64,${png}`);
    const detail = await page.evaluate(() => globalThis.fmdViewFixtureEvents.at(-1).detail);
    assert.equal(detail.bytes, Buffer.byteLength(html), "event retains unmodified renderer byte count");
  });

  await test("heading links scroll and focus their own shadow target without changing host history", async page => {
    await mount(page);
    const before = await page.evaluate(() => ({ url: location.href, history: history.length }));
    await page.locator("#view #jump strong").click();
    assert.equal(await focused(page, "view"), "section");
    const after = await page.evaluate(() => ({ url: location.href, history: history.length,
      y: scrollY, top: document.getElementById("view").shadowRoot.getElementById("section").getBoundingClientRect().top,
      hostFocused: document.activeElement === document.getElementById("section") }));
    assert.equal(after.url, before.url); assert.equal(after.history, before.history);
    assert.ok(after.y > 0 && after.top >= -1 && after.top < 700, "shadow heading scrolled into view");
    assert.equal(after.hostFocused, false);
    await page.keyboard.press("Tab");
    assert.equal(await focused(page, "view"), "after-section", "Tab continues from the heading");
    assert.equal(await page.locator("#view #section").getAttribute("tabindex"), null, "temporary focusability is retired on blur");
  });

  await test("native Enter activation reaches footnotes and their backlinks", async page => {
    await mount(page);
    await page.locator("#view #note").focus();
    await page.keyboard.press("Enter");
    assert.equal(await focused(page, "view"), "fn-1");
    await page.locator("#view #back").focus();
    await page.keyboard.press("Enter");
    assert.equal(await focused(page, "view"), "fnref-1");
    assert.equal(page.url(), `${origin}/host/index.html`);
  });

  await test("inline sources keep ordinary URLs while fragment navigation stays inside their component", async page => {
    await mount(page, null);
    assert.deepEqual(await page.evaluate(() => {
      const root = document.getElementById("view").shadowRoot;
      return [root.getElementById("relative").getAttribute("href"), root.getElementById("relative-image").getAttribute("src")];
    }), ["../intro.md#next", "images/chart.png"]);
    await page.locator("#view #jump").click();
    assert.equal(await focused(page, "view"), "section");
    assert.equal(page.url(), `${origin}/host/index.html`);
  });

  await test("modified, download, alternate-target, external and missing links retain default behavior", async page => {
    await mount(page);
    await preventNavigation(page);
    for (const modifiers of [["Control"], ["Meta"], ["Shift"], ["Alt"]]) {
      await page.locator("#view #jump").click({ modifiers });
      assert.equal(await page.evaluate(() => globalThis.fmdViewFixtureClicks.at(-1).prevented), false);
    }
    await page.locator("#view #jump").click({ button: "middle" });
    assert.deepEqual(await page.evaluate(() => globalThis.fmdViewFixtureClicks.at(-1)), { type: "auxclick", prevented: false, button: 1 });
    for (const id of ["blank", "download", "relative", "query", "absolute-external", "mail", "missing"]) {
      await page.locator(`#view #${id}`).click();
      assert.equal(await page.evaluate(() => globalThis.fmdViewFixtureClicks.at(-1).prevented), false, id);
    }
    for (const id of ["absolute-self", "relative-self"]) {
      await page.locator(`#view #${id}`).click();
      assert.equal(await page.evaluate(() => globalThis.fmdViewFixtureClicks.at(-1).prevented), true, `${id} navigates locally`);
      assert.equal(await focused(page, "view"), "section");
    }
  });

  await test("existing tabindex and explicit host cancellation are preserved", async page => {
    await mount(page);
    await page.evaluate(() => document.getElementById("view").shadowRoot.getElementById("section").setAttribute("tabindex", "0"));
    await page.locator("#view #jump").click();
    assert.equal(await focused(page, "view"), "section");
    await page.keyboard.press("Tab");
    assert.equal(await page.locator("#view #section").getAttribute("tabindex"), "0");
    await page.evaluate(() => document.getElementById("view").addEventListener("click", event => event.preventDefault(), { capture: true }));
    await page.locator("#view #jump").click();
    assert.notEqual(await focused(page, "view"), "section", "capture cancellation prevents component navigation");
  });

  await test("the host's default browsing target does not become local navigation", async page => {
    await mount(page);
    await preventNavigation(page);
    await page.evaluate(() => {
      const base = document.createElement("base"); base.target = "_blank"; document.head.append(base);
    });
    await page.locator("#view #jump").click();
    assert.equal(await page.evaluate(() => globalThis.fmdViewFixtureClicks.at(-1).prevented), false);
    await page.evaluate(() => document.getElementById("view").shadowRoot.getElementById("jump").setAttribute("target", "_self"));
    await page.locator("#view #jump").click();
    assert.equal(await focused(page, "view"), "section");
  });

  await test("fragments decode once and use literal IDs without CSS selector interpretation", async page => {
    await mount(page);
    for (const [link, target] of [["unicode", "café"], ["percent", "literal%20"], ["malformed", "bad%escape"]]) {
      await page.locator(`#view #${link}`).click();
      assert.equal(await focused(page, "view"), target);
    }
  });

  await test("two components with equal IDs never navigate each other's targets", async page => {
    await mount(page, null, "first"); await mount(page, null, "second");
    await page.locator("#second #jump").click();
    assert.equal(await focused(page, "second"), "section");
    assert.equal(await focused(page, "first"), null);
  });

  await test("superseded render completion cannot replace the current publication's URL base", async page => {
    await mount(page);
    await page.evaluate(() => {
      globalThis.fmdViewFixtureDelay = true;
      document.getElementById("view").setAttribute("src", "/old/source.md");
    });
    await page.waitForFunction(() => globalThis.fmdViewFixtureRenders.some(call => call.markdown === "/old/source.md"));
    await page.evaluate(() => document.getElementById("view").setAttribute("src", "/new/source.md"));
    await page.waitForFunction(() => globalThis.fmdViewFixtureRenders.some(call => call.markdown === "/new/source.md"));
    await page.evaluate(() => { const call = globalThis.fmdViewFixtureRenders.find(call => call.markdown === "/new/source.md"); call.resolve(call.output); });
    await page.waitForFunction(() => globalThis.fmdViewFixtureEvents.length === 2);
    await page.evaluate(() => { const call = globalThis.fmdViewFixtureRenders.find(call => call.markdown === "/old/source.md"); call.resolve(call.output); });
    assert.equal(await page.locator("#view #jump").getAttribute("href"), `${origin}/new/source.md#section`);
    await page.locator("#view #jump").click();
    assert.equal(await focused(page, "view"), "section");
    assert.equal(await page.evaluate(() => globalThis.fmdViewFixtureEvents.length), 2);
  });

  await test("failed new source retains the old publication's working navigation", async page => {
    await mount(page);
    await page.evaluate(() => {
      globalThis.fmdViewFixtureDelay = true;
      document.getElementById("view").setAttribute("src", "/new/source.md");
    });
    await page.waitForFunction(() => globalThis.fmdViewFixtureRenders.some(call => call.markdown === "/new/source.md"));
    await page.evaluate(() => globalThis.fmdViewFixtureRenders.at(-1).reject(new Error("Deliberate new-source failure")));
    await page.waitForFunction(() => globalThis.fmdViewFixtureErrors.length === 1);
    assert.deepEqual(await page.evaluate(() => globalThis.fmdViewFixtureErrors), [{ error: "Error: Deliberate new-source failure" }]);
    assert.equal(await page.locator("#view #jump").getAttribute("href"), `${sourceUrl}#section`);
    await page.locator("#view #jump").click();
    assert.equal(await focused(page, "view"), "section");
    assert.equal(await page.evaluate(() => globalThis.fmdViewFixtureEvents.length), 1);
    await page.evaluate(() => { globalThis.fmdViewFixtureErrors = []; }); // Expected error accounted for above.
  });

  await test("opaque data sources retain host-relative resources and local fragment navigation", async page => {
    await mount(page, "data:text/plain,Opaque%20source");
    assert.equal(await page.locator("#view #relative").getAttribute("href"), "../intro.md#next");
    assert.equal(await page.locator("#view #relative-image").getAttribute("src"), "images/chart.png");
    await page.locator("#view #jump").click();
    assert.equal(await focused(page, "view"), "section");
    assert.equal(page.url(), `${origin}/host/index.html`);
  });
} finally {
  await browser.close();
  await new Promise(yes => server.close(yes));
}
const result = { componentSha256: createHash("sha256").update(component).digest("hex"),
  evidence: "production component and real Chromium; explicit renderer boundary fixtures; no generated WASM",
  passed: reports.filter(item => item.pass).length, failed: reports.filter(item => !item.pass).length, reports };
console.log(JSON.stringify(result, null, 2));
if (result.failed) process.exitCode = 1;
