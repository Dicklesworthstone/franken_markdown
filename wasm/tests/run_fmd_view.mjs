// Run with developer-installed Playwright and Chromium, for example:
// node wasm/tests/run_fmd_view.mjs --chromium /path/to/chromium
// No browser or package dependencies are downloaded or added. The local module
// substitution is solely an async boundary oracle for component lifecycle;
// this suite makes no Rust/WASM execution or rendering-quality claim.
import { readFile } from "node:fs/promises";
import { resolve } from "node:path";
import { pathToFileURL } from "node:url";

const { chromium } = await import(process.env.FMD_PLAYWRIGHT_MODULE
  ? pathToFileURL(resolve(process.env.FMD_PLAYWRIGHT_MODULE)).href : "playwright");
const args = process.argv.slice(2);
if (args.length && (args.length !== 2 || args[0] !== "--chromium")) {
  throw new Error("usage: node wasm/tests/run_fmd_view.mjs [--chromium /path/to/chromium]");
}
const component = await readFile(new URL("../fmd-view.js", import.meta.url), "utf8");
const suite = await readFile(new URL("./fmd_view_lifecycle.browser.mjs", import.meta.url), "utf8");
const font = (await readFile(new URL("../../fmd-font/fonts/computer-modern/cmunrm.ttf", import.meta.url))).toString("base64");
const browser = await chromium.launch({
  executablePath: args[1] ?? process.env.FMD_CHROMIUM, headless: true,
  args: ["--no-sandbox", "--disable-dev-shm-usage"],
});
let result;
try {
  const page = await browser.newPage({ userAgent: "franken_markdown-agent (+https://github.com/Dicklesworthstone/franken_markdown)" });
  const errors = [];
  page.on("pageerror", error => errors.push(String(error)));
  await page.route("**/*", route => route.abort());
  await page.setContent('<pre id="result">Running</pre>');
  await page.evaluate(async ({ component, suite, font }) => {
    globalThis.fmdViewTestFont = `data:font/ttf;base64,${font}`;
    const urls = [];
    const moduleUrl = source => {
      const url = URL.createObjectURL(new Blob([source], { type: "text/javascript" }));
      urls.push(url);
      return url;
    };
    try {
      const boundary = moduleUrl("export const createRenderer = (...args) => globalThis.fmdViewTestBoundary.createRenderer(...args);");
      const view = moduleUrl(component.replace('"./franken_markdown.js"', JSON.stringify(boundary)));
      await import(moduleUrl(suite.replace('"../fmd-view.js"', JSON.stringify(view))));
    } finally {
      for (const url of urls) URL.revokeObjectURL(url);
    }
  }, { component, suite, font });
  result = JSON.parse(await page.locator("#result").innerText());
  result.browserErrors = errors;
  console.log(JSON.stringify(result, null, 2));
} finally {
  await browser.close();
}
if (result.failed || result.browserErrors.length) process.exitCode = 1;
