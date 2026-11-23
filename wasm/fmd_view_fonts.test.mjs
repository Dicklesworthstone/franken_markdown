// Exercise the production shadow-document adapter with explicit CSSOM/font
// boundaries. The fixture stacks are the core emitter's actual family order;
// these tests establish binding/ownership, not browser glyph rasterization.
import assert from "node:assert/strict";
import test from "node:test";
import vm from "node:vm";
import { readFile } from "node:fs/promises";

const component = await readFile(new URL("./fmd-view.js", import.meta.url), "utf8");
const entry = new vm.SourceTextModule(component + "\nexport { shadowDocument };", {
  context: vm.createContext({ HTMLElement: class {}, URL }),
});
await entry.link(() => new vm.SyntheticModule(["createRenderer"], function () {
  this.setExport("createRenderer", () => { throw new Error("Unexpected renderer initialization"); });
}, { context: entry.context }));
await entry.evaluate();

function style(values) {
  const fields = new Map(Object.entries(values));
  return {
    getPropertyValue: key => fields.get(key) ?? "",
    setProperty: (key, value) => fields.set(key, value),
  };
}
async function adapt({ body, mono, families = ['"FMD Body"', '"FMD Mono"', '"FMD CJK"'] }) {
  const loaded = [], faces = families.map((family, i) => ({
    type: 5,
    style: style({ "font-family": family, src: `url(data:font/woff;base64,face${i})`,
      "font-style": "normal", "font-weight": "400" }),
    get cssText() { return `@font-face{font-family:${this.style.getPropertyValue("font-family")}}`; },
  }));
  const variables = { selectorText: ":root", style: style({
    "--fmd-font-body": body, "--fmd-font-mono": mono,
  }), get cssText() { return this.selectorText; } };
  const rules = [...faces, variables];
  const sheetNode = { textContent: "renderer stylesheet" };
  const fragment = { querySelectorAll: query => query === "style" ? [sheetNode] : [] };
  const doc = {
    createElement: () => ({ content: fragment }),
    defaultView: {
      CSSStyleSheet: class {
        cssRules = rules;
        replaceSync(value) { assert.equal(value, "renderer stylesheet"); }
      },
      FontFace: class {
        constructor(family, source, descriptors) { Object.assign(this, { family, source, descriptors }); }
        async load() { loaded.push(this); return this; }
      },
    },
  };
  const output = await entry.namespace.shadowDocument("<p>日本語</p>", doc, null);
  return { output, loaded, variables: variables.style, aliases: new Map(
    families.map((family, index) => [family, faces[index].style.getPropertyValue("font-family")]),
  ) };
}

test("shadow typography binds embedded CJK fallback in both body and code stacks", async () => {
  const value = await adapt({
    body: '"FMD Body", "FMD CJK", system-ui, sans-serif',
    mono: '"FMD Mono", "FMD CJK", ui-monospace, monospace',
  });
  const alias = name => value.aliases.get(`"FMD ${name}"`);
  assert.equal(value.variables.getPropertyValue("--fmd-font-body"),
    `${alias("Body")}, ${alias("CJK")}, system-ui, sans-serif`);
  assert.equal(value.variables.getPropertyValue("--fmd-font-mono"),
    `${alias("Mono")}, ${alias("CJK")}, ui-monospace, monospace`);
  assert.equal(value.loaded.length, 3);
  assert.deepEqual(value.loaded.map(face => face.family), [...value.aliases.values()]);
  assert.deepEqual(Array.from(value.output.faces), value.loaded);
});

test("font stack rewriting preserves quoted commas, escaped quotes, comments and unowned names", async () => {
  const fallback = String.raw`"Family, with comma", "Quote\" Name", /* keep , "FMD CJK", */ sans-serif`;
  const value = await adapt({
    body: ` "FMD Body" , ${fallback}, "FMD CJK" `,
    mono: String.raw`'Other, Family', Unowned\,Name, "FMD Mono", "FMD CJK", monospace`,
  });
  assert.equal(value.variables.getPropertyValue("--fmd-font-body"),
    ` ${value.aliases.get('"FMD Body"')} , ${fallback}, ${value.aliases.get('"FMD CJK"')} `);
  assert.equal(value.variables.getPropertyValue("--fmd-font-mono"),
    String.raw`'Other, Family', Unowned\,Name, ${value.aliases.get('"FMD Mono"')}, ${value.aliases.get('"FMD CJK"')}, monospace`);
});

test("every embedded family remains isolated when documents use different CJK subsets", async () => {
  const options = { body: '"FMD Body", "FMD CJK", sans-serif', mono: '"FMD Mono", "FMD CJK", monospace' };
  const first = await adapt(options), second = await adapt(options);
  for (const name of options.body.match(/"[^"]+"/g)) {
    assert.notEqual(first.aliases.get(name), second.aliases.get(name));
    assert.ok(first.variables.getPropertyValue("--fmd-font-body").includes(first.aliases.get(name)));
    assert.ok(second.variables.getPropertyValue("--fmd-font-body").includes(second.aliases.get(name)));
  }
});

test("non-CJK and fallback-only stacks keep their established family order", async () => {
  const value = await adapt({ body: '"FMD Body", system-ui, sans-serif', mono: "ui-monospace, monospace",
    families: ['"FMD Body"'] });
  assert.equal(value.variables.getPropertyValue("--fmd-font-body"),
    `${value.aliases.get('"FMD Body"')}, system-ui, sans-serif`);
  assert.equal(value.variables.getPropertyValue("--fmd-font-mono"), "ui-monospace, monospace");
});
