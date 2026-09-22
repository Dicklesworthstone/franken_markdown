// Actual demo controls + controller + semantic reader. Native engine and Canvas
// are explicit doubles; this is not a generated-WASM/typography proof.

import { createPreviewController } from "../demo/flow_preview_controller.mjs";
import { createReadingControls } from "../demo/flow_reading_controls.mjs";
import { readFlowDocument } from "../flow-reader.js";
import { box, node, ReadingSession } from "./flow_reading_fixtures.mjs";

const assert = (value, message) => {
  if (!value) throw new Error(message);
};
const sourceText = "# Guide\n\nA😀 needle and needle.\n";
export async function run() {
  const results = [],
    el = (id) => document.getElementById(id);
  async function test(name, fn) {
    const source = el("source"),
      query = el("find-text"),
      root = el("reading");
    source.value = sourceText;
    query.value = "";
    el("find-insensitive").checked = false;
    let session, controller, controls;
    const locations = [];
    controls = createReadingControls({
      root,
      panel: el("reader-panel"),
      query,
      insensitive: el("find-insensitive"),
      previous: el("find-previous"),
      next: el("find-next"),
      outline: el("outline"),
      sourceButton: el("reading-source"),
      status: el("reading-status"),
      sourceEditor: source,
      getLocation: (index, snapshot) => controller.locateReading(index, snapshot, source.value),
      onNavigate: (location) => locations.push(location),
    });
    controller = createPreviewController({
      readDocument: readFlowDocument,
      async createSession(text, layout) {
        session = new ReadingSession([
          node("heading", "Guide", { level: 1, enclosingSourceSpan: { startByte: 0, endByte: 8 } }),
          node("paragraph", "A😀 needle and needle.\n", {
            bounds: box(500),
            enclosingSourceSpan: { startByte: 9, endByte: new TextEncoder().encode(text).length },
          }),
        ]);
        session.layoutOptions = layout;
        session.dispose = () => {
          session.disposed = true;
        };
        const reflow = session.reflow.bind(session);
        session.reflow = (opts) => {
          session.layoutOptions = { ...session.layoutOptions, ...opts };
          reflow();
        };
        session.replaceSource = async (value) => {
          session.edit();
          session.roots = [
            node("paragraph", value, {
              enclosingSourceSpan: {
                startByte: 0,
                endByte: new TextEncoder().encode(value).length,
              },
            }),
          ];
        };
        return session;
      },
      painter: {
        async render(s, options) {
          assert(options.token.layoutRevision === s.token.layoutRevision, "reading/pixel fence");
          return { ...s.token, width: options.width, height: options.height };
        },
        clear() {},
        dispose() {},
      },
      onState: (state) => controls.update(state),
    });
    const update = async (more = {}) => {
      controller.update({ source: source.value, width: 400, height: 300, ...more });
      await controller.whenIdle();
      await controls.whenIdle();
    };
    try {
      await update();
      await fn({ source, query, root, session, controller, controls, locations, update });
      results.push(name);
    } finally {
      controller.dispose();
      controls.dispose();
    }
  }
  await test("demo search buttons select native text, cycle matches and map enclosing original Unicode source", async (f) => {
    assert(
      el("outline").options.length === 2 && f.root.querySelector("h1").textContent === "Guide",
      "heading inventory",
    );
    f.query.value = "needle";
    f.query.dispatchEvent(new Event("input"));
    await f.controls.whenIdle();
    assert(
      !el("find-next").disabled && el("reading-status").textContent.startsWith("0 of 2"),
      "search counts",
    );
    el("find-next").click();
    assert(document.getSelection().toString() === "needle", "native match selection");
    assert(f.locations[0].bounds.y === 500, "measured Canvas destination");
    el("find-next").click();
    assert(el("reading-status").textContent.startsWith("2 of 2"), "next result");
    el("find-previous").click();
    assert(el("reading-status").textContent.startsWith("1 of 2"), "previous result");
    el("reading-source").click();
    assert(
      f.source.value.slice(f.source.selectionStart, f.source.selectionEnd) ===
        "A😀 needle and needle.\n",
      "actual enclosing source block, not matched phrase",
    );
    assert(f.source.value === sourceText, "never edits source");
  });
  await test("heading controls navigate measured layout and keyboard search is scoped to the search field", async (f) => {
    el("outline").value = "0";
    el("outline").dispatchEvent(new Event("change"));
    assert(f.locations.length === 1 && f.locations[0].nodeIndex === 0, "heading location");
    f.query.value = "needle";
    f.query.dispatchEvent(new Event("input"));
    await f.controls.whenIdle();
    const event = new KeyboardEvent("keydown", { key: "Enter", shiftKey: true, cancelable: true });
    f.query.dispatchEvent(event);
    assert(
      event.defaultPrevented && el("reading-status").textContent.startsWith("2 of 2"),
      "Shift Enter selects last match",
    );
    assert(document.getSelection().toString() === "needle", "keyboard native selection");
  });
  await test("unsent textarea edits cannot steer source selection or Canvas through old search controls", async (f) => {
    f.query.value = "needle";
    f.query.dispatchEvent(new Event("input"));
    await f.controls.whenIdle();
    f.source.value = "unsubmitted edit";
    el("find-next").click();
    assert(
      f.locations.length === 0 && el("reading-status").textContent.includes("STALE_REVISION"),
      "stale navigation refused",
    );
    assert(f.source.value === "unsubmitted edit", "source preserved");
  });
  await test("scroll-only paints preserve the selected DOM; reflow rebuilds current-token navigation", async (f) => {
    f.query.value = "needle";
    f.query.dispatchEvent(new Event("input"));
    await f.controls.whenIdle();
    el("find-next").click();
    const before = f.root.firstChild,
      snapshot = f.controller.state.document;
    await f.update({ scrollY: 60 });
    assert(
      f.root.firstChild === before && document.getSelection().toString() === "needle",
      "selection survives scrolling",
    );
    await f.update({ width: 800 });
    assert(
      f.root.firstChild !== before && f.controller.state.document !== snapshot,
      "new layout snapshot",
    );
    el("find-next").click();
    assert(
      f.locations.at(-1).layoutRevision === f.session.token.layoutRevision,
      "fresh search geometry",
    );
  });
  await test("reading-budget failure disables only the semantic controls; later source recovers", async (f) => {
    f.session.roots[1].text = "x".repeat(1048577);
    f.session.reflow({});
    await f.update({ scrollY: 10 });
    assert(
      f.controller.state.status === "ready" &&
        f.controller.state.readingError.code === "READING_LIMIT",
      "Canvas remains ready",
    );
    assert(
      f.root.childNodes.length === 0 && el("find-next").disabled,
      "unavailable reader is explicit",
    );
    f.source.value = "recovered";
    await f.update();
    assert(
      f.root.textContent === "recovered" && f.controller.state.readingError === null,
      "new source recovers",
    );
  });
  await test("teardown removes listeners and reader-owned DOM without manipulating source", async (f) => {
    f.controls.dispose();
    f.query.value = "needle";
    f.query.dispatchEvent(new Event("input"));
    await f.controls.whenIdle();
    assert(
      f.root.childNodes.length === 0 && el("find-next").disabled && f.source.value === sourceText,
      "disposed controls inert",
    );
  });
  return results;
}
