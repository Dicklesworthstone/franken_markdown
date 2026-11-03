import { createBookPdfProof } from "../book_pdf_proof.mjs";
import { BOOK_PUBLICATION_FIELDS, normalizeBookPublication } from "./book_collection.mjs";

/** Install a native-viewer surface without touching the restricted HTML-site
 * iframe or the publisher's ordinary PDF/EPUB/site download link. */
export function createBookPdfPanel(root) {
  const existing = root.querySelector("#book-pdf-proof");
  if (existing) return existing;
  const publish = root.querySelector("#publish-title")?.parentElement;
  if (!publish?.parentElement) throw new Error("Missing book publication section.");
  const panel = root.createElement("section");
  panel.id = "book-pdf-proof";
  panel.setAttribute("aria-labelledby", "book-pdf-title");
  const make = (tag, id, text, parent = panel) => {
    const node = root.createElement(tag);
    if (id) node.id = id;
    if (text) node.textContent = text;
    parent.appendChild(node);
    return node;
  };
  make("h2", "book-pdf-title", "Review the actual book PDF");
  make("p", "book-pdf-help", "Generate the complete paginated PDF with the current chapters, includes, book settings, images and supplied fonts. This is the Rust book PDF, not a print of the HTML preview. Download this proof saves the same bytes without rendering again.");
  const actions = make("div");
  for (const [id, text] of [["build", "Generate book PDF proof"], ["cancel", "Cancel PDF rendering"], ["release", "Clear PDF proof"]]) {
    const node = make("button", `book-pdf-${id}`, text, actions);
    node.type = "button";
    node.setAttribute("aria-describedby", "book-pdf-help book-pdf-status");
  }
  const download = make("a", "book-pdf-download", "Download this PDF proof", actions);
  download.hidden = true;
  download.setAttribute("aria-describedby", "book-pdf-status");
  const status = make("p", "book-pdf-status", "Generate a proof after adding chapters. No file is saved automatically.");
  status.setAttribute("role", "status");
  status.setAttribute("aria-live", "polite");
  status.style.overflowWrap = "anywhere";
  make("p", "book-pdf-viewer-help", "Embedded PDF viewing depends on your browser. A blank or unavailable viewer does not prevent downloading the exact proof for inspection in your PDF viewer. No remote viewer or service is loaded. The proof is released on edits or page suspension; it is not a saved source project.");
  make("div", "book-pdf-viewport");
  publish.parentElement.insertBefore(panel, publish);
  return panel;
}

export function createBookPdfControls({ root, controls, collection, worker,
  urls = URL, navigator = globalThis.navigator }) {
  const panel = createBookPdfPanel(root);
  const ids = ["build", "cancel", "release", "download", "status", "viewport"];
  const el = Object.fromEntries(ids.map(id => [id, root.querySelector(`#book-pdf-${id}`)]));
  if (Object.values(el).some(value => !value)) throw new Error("Missing book PDF proof controls.");
  const session = createBookPdfProof({ controls, collection, worker });
  const unlisten = [];
  let disposed = false, suspended = false, composing = false, operation = null;
  let shown = null, url = null;

  function releaseView() {
    const previous = url;
    url = null;
    shown = null;
    // Detach native viewing and download references before revoking the URL.
    el.download.hidden = true;
    el.download.removeAttribute("href");
    el.download.removeAttribute("download");
    el.viewport.replaceChildren();
    if (previous !== null) urls.revokeObjectURL(previous);
  }
  function active() { return !disposed && !suspended && !composing && !controls.sourceBusy; }
  function refresh() {
    const proof = session.current;
    if (shown && (proof !== shown || !active())) {
      releaseView();
      el.status.textContent = "Book changed. The old PDF proof was released; generate a new one.";
    }
    const enabled = active();
    el.build.disabled = !enabled || operation !== null || session.pending
      || !collection.files.some(file => file.role !== "include");
    el.cancel.disabled = !session.pending;
    el.release.disabled = !shown && !session.pending && !worker.hasRetainedBook;
    el.download.hidden = !enabled || !proof || shown !== proof || url === null;
    panel.setAttribute("aria-busy", String(session.pending));
  }
  function invalidate(message, keepIdle = false) {
    operation = null;
    session.invalidate(message, keepIdle);
    releaseView();
    if (!disposed) { el.status.textContent = message; refresh(); }
  }
  // Describe only the receipt admitted with these exact retained bytes, not
  // the live/draft profile. Emission is not independent conformance validation.
  const emissionStatus = proof => proof.pdfA === "2b"
    ? ` Writer receipt: PDF/A-2b${proof.pdfAStrict ? " with strict policy" : ""}. Not independent conformance validation.` : "";
  function show(proof) {
    if (proof === shown && url !== null) {
      el.status.textContent = `Current PDF proof reused without rendering: ${proof.size.toLocaleString()} bytes. Nothing has been saved yet.`
        + emissionStatus(proof);
      return;
    }
    releaseView();
    let next = null;
    try {
      next = urls.createObjectURL(proof.blob);
      let embedded = false;
      if (navigator?.pdfViewerEnabled !== false) {
        try {
          const viewer = root.createElement("object");
          viewer.id = "book-pdf-viewer";
          viewer.type = "application/pdf";
          viewer.setAttribute("aria-label", "Generated book PDF proof");
          viewer.setAttribute("aria-describedby", "book-pdf-viewer-help");
          viewer.style.cssText = "display:block;width:100%;height:65vh;min-height:320px;border:1px solid #d0d7de";
          viewer.data = next;
          const fallback = root.createElement("p");
          fallback.textContent = "Embedded PDF viewer unavailable. Use Download this PDF proof above.";
          viewer.appendChild(fallback);
          el.viewport.appendChild(viewer);
          embedded = true;
        } catch {
          // CSP/viewer/DOM failure must not strand an otherwise valid export.
          el.viewport.replaceChildren();
        }
      }
      if (session.current !== proof || !active())
        throw Object.assign(new Error("Book changed before the PDF viewer was installed."), { code: "STALE_SOURCE" });
      el.download.href = next;
      el.download.download = proof.filename;
      el.download.textContent = `Download this PDF proof (${proof.size.toLocaleString()} bytes)`;
      shown = proof;
      url = next;
      next = null;
      el.status.textContent = `PDF generated from ${proof.chapters} chapters: ${proof.size.toLocaleString()} bytes. Download this proof preserves exactly this output. Nothing has been saved yet.`
        + (embedded ? " Embedded viewing is provided by your browser." : " Embedded viewing is unavailable; download the PDF to inspect it.")
        + emissionStatus(proof);
      refresh();
    } catch (reason) {
      if (next !== null) urls.revokeObjectURL(next);
      releaseView();
      throw reason;
    }
  }
  async function build() {
    if (!active()) throw Object.assign(new Error("Finish importing or composing text before proofing."), { code: "BOOK_BUSY" });
    if (operation) throw Object.assign(new Error("A PDF proof is already rendering."), { code: "BOOK_BUSY" });
    const ticket = {};
    operation = ticket;
    el.status.textContent = "Generating the complete native book PDF in its own cancellable worker…";
    try {
      const proof = await session.render();
      if (operation !== ticket || !active() || session.current !== proof)
        throw Object.assign(new Error("The book changed; this proof was not displayed."), { code: "STALE_SOURCE" });
      show(proof);
      return proof;
    } catch (reason) {
      if (!disposed && operation === ticket) {
        el.status.textContent = `${reason?.code ?? "PDF_PROOF_ERROR"}: ${String(reason?.message ?? "PDF proof failed.").slice(0, 2048)} Source and other exports are unchanged; retry explicitly.`;
      }
      throw reason;
    } finally {
      if (operation === ticket) operation = null;
      if (!disposed) refresh();
    }
  }
  function guardDownload(event) {
    if (!active() || !shown || session.current !== shown || url === null) {
      event.preventDefault();
      invalidate("Book changed. Generate a current PDF proof before downloading.");
    }
    // This ordinary user-activated anchor neither rebuilds the PDF nor claims
    // that the browser's download request actually saved a file to disk.
  }
  const on = (node, type, listener) => {
    if (!node) return;
    node.addEventListener(type, listener);
    unlisten.push(() => node.removeEventListener(type, listener));
  };
  const unsubscribe = session.subscribe(() => { if (!disposed) refresh(); });
  on(el.build, "click", () => { void build().catch(() => {}); });
  for (const id of ["cancel", "release"]) on(el[id], "click", () => {
    operation = null;
    session.cancel();
    releaseView();
    el.status.textContent = "PDF proof cancelled and released. Source and other exports are unchanged.";
    refresh();
  });
  for (const type of ["click", "auxclick", "contextmenu"]) on(el.download, type, guardDownload);
  on(panel, "focusin", refresh);
  for (const id of ["chapter-source", "chapter-path", "source-role", "chapters", "title", "author", "lang", "font", "dark-mode", "font-scale", "toc", "page-numbers"]) {
    const input = root.querySelector(`#${id}`);
    for (const type of ["input", "change"]) on(input, type, () => {
      invalidate("Source or settings changed. Generate a new book PDF proof.",
        id === "chapter-source" || id === "chapters");
    });
  }
  on(root.querySelector("#chapter-source"), "compositionstart", () => {
    composing = true;
    invalidate("Text composition started. Generate a proof after finishing the edit.");
  });
  on(root.querySelector("#chapter-source"), "compositionend", () => { composing = false; refresh(); });
  releaseView();
  refresh();
  return Object.freeze({
    build,
    suspend() {
      if (disposed) return;
      suspended = true; composing = false; operation = null;
      session.suspend(); releaseView(); refresh();
      el.status.textContent = "Book suspended; the PDF proof was released. Generate again after returning.";
    },
    resume() { if (!disposed) { suspended = false; session.resume(); refresh(); } },
    dispose() {
      if (disposed) return;
      disposed = true; operation = null;
      unsubscribe();
      for (const remove of unlisten) remove();
      releaseView(); session.dispose();
      el.build.disabled = el.cancel.disabled = el.release.disabled = true;
      panel.setAttribute("aria-busy", "false");
    },
  });
}

// Profile authoring shares the already-packaged PDF controls module, not its
// viewer/worker lifetime. Rust still owns typography and template expansion.
const PUBLICATION_NUMBERS = [
  ["baseFontSize", "PDF body size (6–24 pt)"],
  ["headingScale", "PDF heading scale (1.05–2)"],
  ["tableFontSize", "PDF table size (5–24 pt)"],
  ["tocDepth", "Table of contents depth (integer 1–6; PDF and HTML)"],
  ["baselineGridPt", "PDF body baseline spacing (1–144 pt; blank or 0 disables)"],
  ["fitToPages", "PDF page-count target (optional integer; not a guarantee)"],
  ["metadataEpochSeconds", "PDF metadata epoch (nonnegative seconds; 0 is valid)"],
];
const PUBLICATION_FLAGS = [
  ["homogeneous", "Gradual adjacent-line demerits"], ["antiriver", "Reduce whitespace rivers"],
  ["pareto", "Pareto line breaking"], ["optimal-pagination", "Optimal pagination"],
  ["strict-pagination", "Strict pagination (stop export if optimal layout cannot be completed)"],
  ["protrusion", "Margin protrusion"], ["expansion", "Glyph expansion"],
];
const PUBLICATION_BANDS = ["header", "footer"];
const PUBLICATION_SLOTS = ["left", "center", "right"];
const publicationKey = value => JSON.stringify(Object.fromEntries(BOOK_PUBLICATION_FIELDS
  .filter(key => value[key] !== undefined).map(key => [key, value[key]])));
const publicationFailure = (code, message) => Object.assign(new Error(message), { code });
const publicationId = id => `book-publication-${id}`;

/** Independent authoring panel. It never renders, loads assets or grants access. */
export function createBookPublicationPanel(root) {
  const existing = root.querySelector("#book-publication-settings");
  if (existing) return existing;
  const publish = root.querySelector("#publish-title")?.parentElement;
  if (!publish?.parentElement) throw new Error("Missing book publication section.");
  const panel = root.createElement("section");
  panel.id = publicationId("settings");
  panel.setAttribute("aria-labelledby", publicationId("title"));
  const make = (tag, id, text, parent = panel) => {
    const node = root.createElement(tag);
    if (id) node.id = publicationId(id);
    if (text !== undefined) node.textContent = text;
    parent.appendChild(node);
    return node;
  };
  const field = (parent, tag, id, label) => {
    make("label", null, label, parent).setAttribute("for", publicationId(id));
    const node = make(tag, id, undefined, parent);
    node.setAttribute("aria-describedby", `${publicationId("help")} ${publicationId("status")}`);
    return node;
  };
  const check = (parent, id, text) => {
    const label = make("label", null, undefined, parent);
    const node = make("input", id, undefined, label); node.type = "checkbox";
    make("span", null, ` ${text}`, label);
  };
  const select = (parent, id, label, choices) => {
    const node = field(parent, "select", id, label);
    for (const [value, text] of choices) make("option", null, text, node).value = value;
    return node;
  };
  make("h2", "title", "Typography and publishing profile");
  make("p", "help", "Edit a draft, then Apply publishing profile. Only applied settings are used by exports, PDF proofs and saved projects. Applying never edits source text, changes paper geometry, or starts a render. Blank numeric fields use native defaults. Discard reloads the applied profile; page suspension discards drafts.");
  const layout = make("fieldset"); make("legend", null, "Layout and output", layout);
  const grid = make("div", null, undefined, layout); grid.className = "grid";
  for (const [id, label] of PUBLICATION_NUMBERS) {
    const cell = make("div", null, undefined, grid);
    const input = field(cell, "input", id, label);
    // Text avoids number-input sanitization turning invalid typed text into an
    // empty/default value. The shared native contract admits actual numbers.
    input.type = "text"; input.inputMode = "decimal"; input.maxLength = 64;
    input.autocomplete = "off"; input.spellcheck = false; input.placeholder = "Native default";
  }
  make("p", "grid-help", "A positive baseline spacing aligns PDF body text to a shared grid. Clear the page-count target before enabling a grid. Spacing is rounded once by the renderer to 0.001 pt; blank or 0 uses ordinary line spacing.", layout);
  check(layout, "codeLineNumbers", "PDF code line numbers");
  select(layout, "htmlFontFormat", "HTML-site font container (EPUB uses the source font format)",
    [["", "Native default"], ["ttf", "Raw font (.ttf/.otf)"], ["woff1", "WOFF"], ["woff2", "WOFF2"]]);
  select(layout, "fontEmbedding", "HTML/EPUB font embedding",
    [["", "Compact subsets (default)"], ["subset", "Compact subsets"], ["full", "Complete fonts for browser/reader shaping"]]);
  make("p", "font-help", "Complete fonts retain character maps and OpenType layout tables, and increase publication size. PDF font subsetting is unchanged.", layout);
  const archival = make("fieldset"); make("legend", null, "PDF archival emission", archival);
  const policy = select(archival, "pdfPolicy", "PDF output policy", [
    ["off", "Ordinary PDF (default)"], ["2b", "PDF/A-2b emission"],
    ["2b-strict", "PDF/A-2b emission with strict writer policy"],
  ]);
  policy.setAttribute("aria-describedby", `${publicationId("archival-help")} ${publicationId("status")}`);
  make("p", "archival-help", "PDF only: EPUB and HTML output are unchanged. These are native writer settings, not PDF/A validation or certification. Strict writer policy is separate from checking internal book links. Unsupported packages or rejected archival exports fail without falling back to ordinary PDF. Applied archival settings require source-project version 5; older workbenches must be upgraded to reopen those projects.", archival);
  const quality = make("fieldset"); make("legend", null, "Optional native typography", quality);
  make("p", null, "These switches request the native layout algorithms; they do not guarantee a page count or faster rendering. Strict pagination also enables optimal pagination and stops PDF export when that layout cannot be completed. Inspect the actual PDF proof.", quality);
  for (const [token, label] of PUBLICATION_FLAGS) check(quality, `type-${token}`, label);
  make("p", "running-help", "Running headers and footers affect PDF only. Templates accept {title}, {author}, {page} and {pages}; template validation and margin fitting belong to the renderer. Each slot permits 4096 UTF-8 bytes. Logos reference already imported book images, never URLs or implicit file access. Missing references survive source-only restores until reauthorized.");
  for (const band of PUBLICATION_BANDS) {
    const group = make("fieldset"); make("legend", null, `PDF ${band}`, group);
    group.setAttribute("aria-describedby", publicationId("running-help"));
    for (const slot of PUBLICATION_SLOTS) {
      const input = field(group, "textarea", `${band}-${slot}`, `${band} ${slot} template`);
      input.rows = 2; input.maxLength = 4096; input.spellcheck = false;
      input.style.height = "4em";
    }
    check(group, `${band}-rule`, "Draw a rule");
    select(group, `${band}-image`, "Repeated logo (authorized book-relative image path)", [["", "No logo"]]);
    select(group, `${band}-position`, "Logo position", [["", "Native default"], ["left", "Left"], ["right", "Right"]]);
    const height = field(group, "input", `${band}-height`, "Logo height in points (optional integer 1–65535)");
    height.type = "text"; height.inputMode = "numeric"; height.maxLength = 16;
    height.placeholder = "Native default";
  }
  check(panel, "skipFirstPage", "Skip running content on the first PDF page (including the page-number footer)");
  const actions = make("div");
  for (const [id, text] of [["apply", "Apply publishing profile"], ["discard", "Discard draft / reload applied"],
    ["defaults", "Use native defaults in draft"]]) {
    const button = make("button", id, text, actions); button.type = "button";
  }
  const status = make("p", "status", "Loading applied publishing settings.");
  status.setAttribute("role", "status"); status.setAttribute("aria-live", "polite");
  make("p", "resources", "");
  make("h3", null, "Applied profile (saved with the book)");
  const current = make("pre", "current", ""); current.setAttribute("aria-label", "Applied publishing profile");
  make("p", null, "Applying a changed profile clears older previews, proofs and prepared downloads. Save the book to retain it. Source-only saves retain logo paths but exclude image and font bytes; portable backups require explicit resource authorization. A matching WASM build is required to render requested features; unsupported builds fail explicitly without changing source.");
  publish.parentElement.insertBefore(panel, publish);
  return panel;
}

/** Drafts are independent of raw source editing. Apply changes only the model's
 * profile, never captures/rewrites a textarea or starts an export. Existing
 * collection observers invalidate outputs and persist only committed settings. */
export function createBookPublicationControls({ root, controls, collection }) {
  if (typeof collection?.setPublication !== "function" || !controls)
    throw new TypeError("Publishing controls require a current book collection and source host.");
  const panel = createBookPublicationPanel(root);
  const checkIds = ["codeLineNumbers", "skipFirstPage", ...PUBLICATION_FLAGS.map(([token]) => `type-${token}`),
    ...PUBLICATION_BANDS.map(band => `${band}-rule`)];
  const valueIds = [...PUBLICATION_NUMBERS.map(([id]) => id), "htmlFontFormat", "fontEmbedding", "pdfPolicy",
    ...PUBLICATION_BANDS.flatMap(band => [...PUBLICATION_SLOTS, "image", "position", "height"].map(slot => `${band}-${slot}`))];
  const ids = [...checkIds, ...valueIds, "apply", "discard", "defaults", "status", "resources", "current"];
  const el = Object.fromEntries(ids.map(id => [id, root.querySelector(`#${publicationId(id)}`)]));
  if (Object.values(el).some(node => !node)) throw new Error("Missing publishing profile controls.");
  let disposed = false, suspended = false, applying = false, stale = false;
  let baseline = "", signatureAtLoad = "", loadedProfile = {}, loadedViews = new Map();
  const composing = new Set(), logoKeys = new Map(), unlisten = [];
  const configuration = () => collection.renderConfigurationRevision;
  const signature = () => JSON.stringify([...checkIds.map(id => el[id].checked), ...valueIds.map(id => el[id].value)]);
  const blocked = () => disposed || suspended || composing.size > 0 || controls.sourceBusy;
  const check = () => {
    if (disposed || suspended) throw publicationFailure("PROFILE_CLOSED", "Publishing profile controls are not active.");
    if (composing.size || controls.sourceBusy)
      throw publicationFailure("BOOK_BUSY", "Finish importing or composing text before changing the publishing profile.");
  };
  function logos(band, selected) {
    const options = [["", "No logo"], ...collection.images.map(image => [image.destination, image.destination])];
    if (selected && !options.some(([path]) => path === selected)) options.push([selected, `[not authorized] ${selected}`]);
    const key = JSON.stringify([options, selected]);
    if (logoKeys.get(band) === key && el[`${band}-image`].value === selected) return;
    logoKeys.set(band, key);
    el[`${band}-image`].replaceChildren(...options.map(([value, text]) => {
      const option = root.createElement("option"); option.value = value; option.textContent = text; return option;
    }));
    el[`${band}-image`].value = selected;
  }
  function write(value) {
    for (const [id] of PUBLICATION_NUMBERS) el[id].value = value[id] === undefined ? "" : String(value[id]);
    el.codeLineNumbers.checked = value.codeLineNumbers === true;
    el.htmlFontFormat.value = value.htmlFontFormat ?? "";
    el.fontEmbedding.value = value.fontEmbedding ?? "";
    el.pdfPolicy.value = value.pdfA === "2b" ? (value.pdfAStrict ? "2b-strict" : "2b") : "off";
    const flags = new Set((value.typography ?? "").split(",").map(token => token.trim()));
    for (const [token] of PUBLICATION_FLAGS) el[`type-${token}`].checked = flags.has(token);
    el.skipFirstPage.checked = value.running?.skipFirstPage === true;
    for (const band of PUBLICATION_BANDS) {
      const data = value.running?.[band] ?? {};
      for (const slot of PUBLICATION_SLOTS) el[`${band}-${slot}`].value = data[slot] ?? "";
      el[`${band}-rule`].checked = data.rule === true;
      logos(band, data.image?.dest ?? "");
      el[`${band}-position`].value = data.image?.position ?? "";
      el[`${band}-height`].value = data.image?.heightPt === undefined ? "" : String(data.image.heightPt);
    }
  }
  function load() {
    loadedProfile = JSON.parse(publicationKey(collection.options));
    write(loadedProfile);
    loadedViews = new Map(valueIds.map(id => [id, el[id].value]));
    baseline = configuration(); signatureAtLoad = signature(); stale = false;
  }
  function refresh() {
    if (disposed) return;
    if (!applying && configuration() !== baseline) {
      if (signature() === signatureAtLoad && !stale) load();
      else stale = true;
    }
    const disabled = blocked();
    for (const id of [...checkIds, ...valueIds, "defaults", "discard"])
      el[id].disabled = disabled && !(composing.has(id) && !suspended && !controls.sourceBusy);
    el.apply.disabled = disabled || stale;
    for (const band of PUBLICATION_BANDS) {
      const selected = el[`${band}-image`].value;
      logos(band, selected); // Includes unresolved restored paths without granting authority.
      el[`${band}-position`].disabled = disabled || !selected;
      el[`${band}-height`].disabled = !selected || (disabled && !(composing.has(`${band}-height`) && !suspended && !controls.sourceBusy));
    }
    const applied = collection.options;
    el.current.textContent = publicationKey(applied) === "{}" ? "Native defaults (no publishing overrides)."
      : JSON.stringify(JSON.parse(publicationKey(applied)), null, 2);
    const authorized = new Set(collection.images.map(image => image.destination));
    const missing = PUBLICATION_BANDS.map(band => el[`${band}-image`].value)
      .filter((path, index, paths) => path && !authorized.has(path) && paths.indexOf(path) === index);
    el.resources.textContent = missing.length ? `Logo resources not authorized: ${missing.join(", ")}. Import these paths before PDF rendering; saving a reference grants no access.`
      : "Logo choices use the book's current image grants. No resources are loaded by these controls.";
    if (stale) el.status.textContent = "STALE_SOURCE: Book settings, resources or structure changed while this draft was open. Discard/reload the applied profile before editing again; your draft was not applied.";
  }
  function number(id) {
    const raw = el[id].value.trim();
    if (!raw) return undefined;
    if (raw.length > 64 || !/^(?:\d+(?:\.\d*)?|\.\d+)(?:[eE][+-]?\d+)?$/.test(raw))
      throw publicationFailure("INVALID_OPTIONS", `Enter a nonnegative decimal number or leave ${id} blank.`);
    const value = Number(raw);
    if (!Number.isFinite(value)) throw publicationFailure("INVALID_OPTIONS", `${id} is too large.`);
    return value;
  }
  function draft() {
    const value = Object.fromEntries(PUBLICATION_NUMBERS.map(([id]) => [id, number(id)]));
    const originalFlags = new Set((loadedProfile.typography ?? "").split(",").map(token => token.trim()));
    const flagsUnchanged = PUBLICATION_FLAGS.every(([token]) => el[`type-${token}`].checked === originalFlags.has(token));
    value.typography = flagsUnchanged ? loadedProfile.typography
      : PUBLICATION_FLAGS.filter(([token]) => el[`type-${token}`].checked).map(([token]) => token).join(",");
    value.codeLineNumbers = el.codeLineNumbers.checked;
    value.htmlFontFormat = el.htmlFontFormat.value || undefined;
    value.fontEmbedding = el.fontEmbedding.value || undefined;
    // One selection represents both native fields, so the UI cannot stage an
    // impossible OFF/strict combination. Missing/unknown values fail closed.
    const policy = el.pdfPolicy.value;
    if (!["off", "2b", "2b-strict"].includes(policy))
      throw publicationFailure("INVALID_OPTIONS", "Choose a supported PDF output policy.");
    if (policy !== "off") {
      value.pdfA = "2b";
      value.pdfAStrict = policy === "2b-strict";
    }
    const running = { skipFirstPage: el.skipFirstPage.checked };
    for (const band of PUBLICATION_BANDS) {
      const data = Object.fromEntries(PUBLICATION_SLOTS.map(slot => {
        const id = `${band}-${slot}`, text = el[id].value;
        // DOM textareas normalize CRLF. Preserve the exact saved template while
        // its displayed view is unchanged, including edits to unrelated fields.
        return [slot, text === loadedViews.get(id) ? loadedProfile.running?.[band]?.[slot] ?? "" : text];
      }));
      data.rule = el[`${band}-rule`].checked;
      const dest = el[`${band}-image`].value;
      if (dest) data.image = { dest, position: el[`${band}-position`].value || undefined, heightPt: number(`${band}-height`) };
      running[band] = data;
    }
    value.running = running;
    return normalizeBookPublication(value);
  }
  function review() {
    refresh();
    if (disposed || stale) return;
    try {
      const value = draft();
      el.status.textContent = publicationKey(value) === publicationKey(collection.options)
        ? "Draft matches the applied profile. No publishing change is pending."
        : "Unapplied publishing draft. Apply to use it in new exports and saved projects; existing settings are unchanged.";
    } catch (error) {
      el.status.textContent = `${error.code ?? "INVALID_OPTIONS"}: ${error.message} Applied settings are unchanged.`;
    }
  }
  function apply() {
    check(); refresh();
    if (stale) throw publicationFailure("STALE_SOURCE", "Reload the current profile before applying this draft.");
    const selected = signature(), captured = baseline, value = draft();
    check();
    if (selected !== signature() || configuration() !== captured)
      throw publicationFailure("STALE_SOURCE", "The publishing draft changed; nothing was applied.");
    const before = collection.revision;
    let installed;
    applying = true;
    try { installed = collection.setPublication(value, before); }
    finally { applying = false; }
    if (disposed) return installed;
    load(); refresh();
    el.status.textContent = collection.revision !== installed
      ? "The book changed again during application. The displayed applied profile is current; review it before publishing."
      : installed === before ? "Publishing profile was already current; no revision or output changed."
      : "Publishing profile applied. Generate a new PDF proof or export, then save the book to retain these settings. No source text was changed.";
    return installed;
  }
  function discard() { check(); load(); review(); }
  function defaults() {
    check(); load(); write({}); review();
    if (!stale) el.status.textContent = "Native defaults are staged in the draft. Apply to remove publishing overrides; metadata, source, paper and resources will be preserved.";
  }
  const run = fn => {
    try { return fn(); }
    catch (error) {
      if (!disposed) { el.status.textContent = `${error.code ?? "PROFILE_ERROR"}: ${error.message}`; refresh(); }
    }
  };
  const on = (node, type, fn) => {
    node.addEventListener(type, fn); unlisten.push(() => node.removeEventListener(type, fn));
  };
  on(el.apply, "click", () => run(apply)); on(el.discard, "click", () => run(discard)); on(el.defaults, "click", () => run(defaults));
  for (const id of [...checkIds, ...valueIds]) {
    on(el[id], "input", review); on(el[id], "change", review);
    on(el[id], "compositionstart", () => { composing.add(id); refresh(); });
    on(el[id], "compositionend", () => { composing.delete(id); review(); });
  }
  on(panel, "focusin", refresh);
  load(); review();
  const unsubscribe = collection.subscribe(refresh);
  const unsubscribeSource = controls.subscribeSourceState?.(refresh);
  return Object.freeze({ apply, discard, defaults, refresh,
    suspend() {
      if (disposed) return;
      suspended = true; composing.clear(); load(); refresh();
      el.status.textContent = "Publishing draft discarded on suspension. Applied settings remain in the book.";
    },
    resume() { if (!disposed) { suspended = false; load(); review(); } },
    dispose() {
      if (disposed) return;
      disposed = true; composing.clear(); unsubscribe(); unsubscribeSource?.();
      for (const remove of unlisten) remove();
      for (const id of [...checkIds, ...valueIds, "apply", "discard", "defaults"]) el[id].disabled = true;
    },
  });
}
