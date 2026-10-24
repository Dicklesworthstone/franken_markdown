// Actual component/facade and caller-supplied generated renderer. A historical
// package proves that artifact's compatibility, not current-source WASM parity.
import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { mkdir, readFile } from "node:fs/promises";
import { resolve, sep, extname } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";
assert.ok(process.env.FMD_WASM_PKG, "FMD_WASM_PKG must name existing generated bindings");
const root = fileURLToPath(new URL("../", import.meta.url)), pkg = resolve(process.env.FMD_WASM_PKG);
const output = process.env.FMD_BROWSER_OUTPUT ?? "/tmp/fmd-view-typography";
await mkdir(output, { recursive: true });
const generated = {};
for (const name of ["franken_markdown.js", "franken_markdown_bg.wasm"]) {
  const bytes = await readFile(resolve(pkg, name));
  generated[name] = { bytes: bytes.length, sha256: createHash("sha256").update(bytes).digest("hex") };
}
const { chromium } = await import(process.env.FMD_PLAYWRIGHT_MODULE
  ? pathToFileURL(resolve(process.env.FMD_PLAYWRIGHT_MODULE)).href : "playwright");
const browser = await chromium.launch({ headless: true,
  ...(process.env.FMD_CHROMIUM ? { executablePath: process.env.FMD_CHROMIUM } : {}) });
const context = await browser.newContext({ viewport: { width: 1280, height: 1000 },
  userAgent: "franken_markdown-agent (+https://github.com/Dicklesworthstone/franken_markdown)" });
const checks = [], errors = [], denied = [];
const check = (name, actual, expected) => { assert.deepEqual(actual, expected, name); checks.push(name); };
let page;
try {
  await context.route("**/*", async route => {
    const url = new URL(route.request().url());
    if (url.origin !== "https://fmd-tests.invalid" || !url.pathname.startsWith("/wasm/")) {
      denied.push(url.href); return route.abort();
    }
    const relative = decodeURIComponent(url.pathname.slice(6));
    const base = relative.startsWith("pkg/") ? pkg : root;
    const path = resolve(base, relative.startsWith("pkg/") ? relative.slice(4) : relative);
    if (!path.startsWith(base.replace(/\/$/, "") + sep)) { denied.push(url.href); return route.abort(); }
    try {
      await route.fulfill({ body: await readFile(path), contentType: ({ ".html": "text/html",
        ".js": "text/javascript", ".mjs": "text/javascript", ".wasm": "application/wasm", ".md": "text/plain" })[extname(path)] ?? "application/octet-stream" });
    } catch (error) { errors.push(`${url.pathname}: ${error.code}`); await route.abort(); }
  });
  page = await context.newPage(); page.setDefaultTimeout(10000);
  page.on("pageerror", error => errors.push(error.message));
  await page.goto("https://fmd-tests.invalid/wasm/demo/web-component.html");
  await page.waitForFunction(() => document.querySelector("#inline").shadowRoot.querySelector("p"));
  const observations = await page.evaluate(async () => {
    for (const element of document.querySelectorAll("fmd-view")) element.remove();
    document.body.replaceChildren();
    const css = document.createElement("style");
    css.textContent = "body{font:13px monospace;color:rgb(201,2,3);margin:16px}p,h1{color:rgb(201,2,3)}fmd-view{display:block;border:1px solid #ccc;margin:16px 0}";
    document.head.append(css);
    const control = document.createElement("p"); control.id = "host-control"; control.textContent = "Host retains its own typography."; document.body.append(control);
    const source = "# Embedded typography\n\nProse café with **bold**, *italic* and `code`.\n\n> A quotation.\n\n| A | B |\n|---|---|\n| one | two |";
    const { renderHtml } = await import("/wasm/franken_markdown.js");
    const pairs = [];
    for (const [font, darkMode] of [["sans", "auto"], ["serif", "disabled"]]) {
      const element = document.createElement("fmd-view"); element.id = font;
      element.setAttribute("font", font); element.setAttribute("dark-mode", darkMode); element.textContent = source;
      const events = [], event = new Promise(resolve => element.addEventListener("fmd-rendered", resolve, { once: true }));
      element.addEventListener("fmd-rendered", event => events.push(event.detail));
      element.addEventListener("fmd-error", event => { throw new Error(event.detail.error); });
      document.body.append(element); await event;
      const rendered = await renderHtml(source, { font, darkMode });
      const iframe = document.createElement("iframe"); iframe.setAttribute("sandbox", "allow-same-origin");
      iframe.style.cssText = `position:absolute;left:-10000px;visibility:hidden;width:${element.clientWidth}px`;
      const loaded = new Promise(resolve => iframe.onload = resolve); iframe.srcdoc = rendered.text(); document.body.append(iframe); await loaded;
      await iframe.contentDocument.fonts.ready;
      pairs.push({ element, iframe, events, expected: { bytes: rendered.bytes.byteLength,
        sourceLength: rendered.sourceLength, diagnostics: rendered.diagnostics } });
    }
    const style = pairs[0].element.shadowRoot.querySelectorAll("style")[1];
    const fontRule = [...style.sheet.cssRules].find(rule => rule.type === 5);
    const ownerFace = await new FontFace("host-owned", fontRule.style.getPropertyValue("src")).load();
    document.fonts.add(ownerFace);
    window.typographyPairs = pairs; window.typographyOwnerFace = ownerFace;
    window.typographyStyles = () => pairs.map(({ element, iframe }) => {
      const host = getComputedStyle(element), body = iframe.contentWindow.getComputedStyle(iframe.contentDocument.body);
      const properties = ["color", "backgroundColor", "fontSize", "lineHeight", "fontFeatureSettings"];
      const pick = value => Object.fromEntries(properties.map(key => [key, value[key]]));
      const width = node => { const range = document.createRange(); range.selectNodeContents(node); return range.getBoundingClientRect().width; };
      const paragraph = element.shadowRoot.querySelector("main > p"), standalone = iframe.contentDocument.querySelector("main > p");
      return { host: pick(host), body: pick(body), base: host.getPropertyValue("--fmd-base"),
        family: getComputedStyle(paragraph).fontFamily, mono: getComputedStyle(paragraph.querySelector("code")).fontFamily,
        proseWidth: width(paragraph), standaloneProseWidth: width(standalone),
        codeWidth: width(paragraph.querySelector("code")), standaloneCodeWidth: width(standalone.querySelector("code")) };
    });
    return { styles: typographyStyles(), details: pairs.map(pair => ({ actual: pair.events[0], expected: pair.expected })),
      faces: [...document.fonts].map(face => ({ family: face.family, status: face.status })), hostColor: getComputedStyle(control).color };
  });
  for (const [index, item] of observations.styles.entries()) {
    check(`view ${index} body typography matches standalone`, item.host, item.body);
    check(`view ${index} theme base`, item.base.trim(), "16px");
    assert.match(item.family, /^fmd-view-/); checks.push(`view ${index} uses its owned embedded body family`);
    assert.match(item.mono, /^fmd-view-/); checks.push(`view ${index} uses its owned embedded mono family`);
    check(`view ${index} embedded prose metrics match standalone`, item.proseWidth, item.standaloneProseWidth);
    check(`view ${index} embedded code metrics match standalone`, item.codeWidth, item.standaloneCodeWidth);
    check(`view ${index} retains original renderer event bytes and diagnostics`, observations.details[index].actual, observations.details[index].expected);
  }
  check("host page styles remain unchanged", observations.hostColor, "rgb(201, 2, 3)");
  check("all registered embedded faces are decoded", observations.faces.every(face => face.status === "loaded"), true);
  assert.notEqual(observations.styles[0].family, observations.styles[1].family); checks.push("sans and serif instances have distinct family authority");
  await page.emulateMedia({ colorScheme: "dark" });
  const dark = await page.evaluate(() => typographyStyles());
  for (const [index, item] of dark.entries()) check(`view ${index} dark-mode policy matches standalone`, item.host, item.body);
  assert.notEqual(dark[0].host.color, dark[1].host.color); checks.push("auto dark and disabled dark coexist");
  await page.emulateMedia({ media: "print" });
  for (const [index, item] of (await page.evaluate(() => typographyStyles())).entries())
    check(`view ${index} print typography matches standalone`, item.host, item.body);
  await page.emulateMedia({ media: "screen", colorScheme: "light" });
  const lifetime = await page.evaluate(async () => {
    const [a, b] = typographyPairs;
    const own = element => new Set([...element.shadowRoot.querySelectorAll("style")].flatMap(style =>
      [...style.sheet.cssRules].filter(rule => rule.type === 5).map(rule => rule.style.getPropertyValue("font-family").replaceAll('"', ""))));
    const old = [...document.fonts].filter(face => own(a.element).has(face.family));
    const peer = [...document.fonts].filter(face => own(b.element).has(face.family));
    const updated = new Promise(resolve => a.element.addEventListener("fmd-rendered", resolve, { once: true }));
    a.element.textContent = "# New subset\n\nChanged Ω café."; await updated;
    const replacementRetiresOnlyOwn = old.length > 0 && peer.length > 0
      && old.every(face => !document.fonts.has(face)) && peer.every(face => document.fonts.has(face));
    a.element.remove();
    const disconnectKeepsPeer = peer.length > 0 && peer.every(face => document.fonts.has(face)) && document.fonts.has(typographyOwnerFace);
    const resumed = new Promise(resolve => a.element.addEventListener("fmd-rendered", resolve, { once: true }));
    document.body.append(a.element); await resumed;
    const resumedFaces = [...document.fonts].filter(face => own(a.element).has(face.family));
    const reconnectLoaded = resumedFaces.length > 0 && resumedFaces.every(face => face.status === "loaded");
    return { replacementRetiresOnlyOwn, disconnectKeepsPeer, reconnectLoaded };
  });
  for (const [name, actual] of Object.entries(lifetime)) check(name, actual, true);
  await page.screenshot({ path: resolve(output, "desktop.png"), fullPage: true });
  await page.setViewportSize({ width: 390, height: 844 });
  await page.screenshot({ path: resolve(output, "mobile.png"), fullPage: true });
  check("mobile has no horizontal overflow", await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth), true);
  check("final disconnect retains only unrelated host font", await page.evaluate(() => {
    for (const { element } of typographyPairs) element.remove();
    return document.fonts.size === 1 && document.fonts.has(typographyOwnerFace);
  }), true);
  check("browser errors", errors, []); check("unapproved network", denied, []);
  console.log(JSON.stringify({ browser: browser.version(), pkg, generated, checks: checks.length, names: checks, output }, null, 2));
} catch (error) { console.error(JSON.stringify({ completedChecks: checks, errors, denied }, null, 2)); throw error; }
finally { await context.close(); await browser.close(); }
