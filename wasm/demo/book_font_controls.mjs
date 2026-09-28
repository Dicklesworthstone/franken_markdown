import { BOOK_FONT_SLOTS } from "./book_font_assets.mjs";
import { createBookFontAuthoring } from "./book_font_authoring.mjs";

const LABELS = ["Body regular", "Body bold", "Body italic", "Body bold italic", "Monospace"];
const failure = (code, message) => Object.assign(new Error(message), { code });

/** Build only application-owned elements. Font names and failures always use
 * textContent, never HTML or CSS. No browser FontFace or font URL is installed. */
export function createBookFontPanel(root) {
  if (root.querySelector("#book-font-panel")) return;
  const publish = root.querySelector("#publish-title")?.parentElement;
  if (!publish?.parentElement) throw new Error("Book publisher section is missing.");
  const make = (tag, text, parent, id) => {
    const node = root.createElement(tag);
    if (id) node.id = id;
    if (text !== undefined) node.textContent = text;
    if (parent) parent.appendChild(node);
    return node;
  };
  const panel = make("section", undefined, null, "book-font-panel");
  panel.setAttribute("aria-labelledby", "book-font-title");
  make("h2", "Book fonts", panel, "book-font-title");
  make("p", "Assign local TrueType (.ttf) files to any of the five publishing roles. Choose one or several files, then validate and assign the batch. Files are checked by the existing native PDF engine before replacing any font. This works before adding a chapter; it does not certify whole-book glyph coverage.", panel, "book-font-help");
  make("p", "Blank weight uses the renderer's role default. A weight pin must be an integer from 1 through 1000 and is interpreted by the engine for variable fonts. Reselect a file to change its pin. Unassigned roles use the engine's normal bundled/inherited fallback; changing the Sans/Serif selector does not revoke supplied fonts.", panel, "book-font-weight-help");
  make("p", "Only assign fonts you have permission to embed. Fonts stay in this page session and are included in explicit portable backups, but not source-only projects or local-library saves. A source-only restore or page suspension revokes them. No font is uploaded or installed in your operating system. Limit: 8 MiB per role, 32 MiB total. Save a portable backup before removal; source undo does not restore fonts.", panel, "book-font-save-help");
  make("p", "No supplied fonts.", panel, "book-font-inventory");
  for (let i = 0; i < BOOK_FONT_SLOTS.length; i++) {
    const slot = BOOK_FONT_SLOTS[i], group = make("fieldset", undefined, panel);
    group.style.cssText = "min-width:0;margin:12px 0;padding:12px;border:1px solid #d0d7de;border-radius:6px";
    make("legend", LABELS[i], group);
    const current = make("p", "No supplied font; normal engine fallback.", group, `book-font-current-${slot}`);
    current.style.overflowWrap = "anywhere";
    const fileLabel = make("label", "Assign or replace font file", group);
    fileLabel.setAttribute("for", `book-font-file-${slot}`);
    const file = make("input", undefined, group, `book-font-file-${slot}`);
    file.type = "file"; file.accept = ".ttf,font/ttf";
    file.setAttribute("aria-describedby", `book-font-current-${slot} book-font-help book-font-save-help`);
    const weightLabel = make("label", "Optional weight pin", group);
    weightLabel.setAttribute("for", `book-font-weight-${slot}`);
    const weight = make("input", undefined, group, `book-font-weight-${slot}`);
    weight.type = "text"; weight.inputMode = "numeric"; weight.maxLength = 4;
    weight.placeholder = "Role default"; weight.autocomplete = "off";
    weight.setAttribute("aria-describedby", "book-font-weight-help");
    const remove = make("button", "Remove supplied font", group, `book-font-remove-${slot}`);
    remove.type = "button"; remove.disabled = true;
  }
  const actions = make("p", undefined, panel);
  for (const [id, text] of [
    ["book-font-assign", "Validate and assign selected fonts"],
    ["book-font-cancel", "Cancel font operation"],
    ["book-font-discard", "Discard selected files"],
    ["book-font-clear", "Revoke all supplied fonts"],
  ]) {
    const button = make("button", text, actions, id); button.type = "button"; button.disabled = true;
  }
  const status = make("p", "Choose a local font file for any role to begin.", panel, "book-font-status");
  status.setAttribute("role", "status"); status.setAttribute("aria-live", "polite");
  status.style.overflowWrap = "anywhere";
  publish.parentElement.insertBefore(panel, publish);
}

export function createBookFontControls({ root, collection, controls, worker, confirm }) {
  const ids = ["book-font-panel", "book-font-inventory", "book-font-status", "book-font-assign",
    "book-font-cancel", "book-font-discard", "book-font-clear"];
  const el = Object.fromEntries(ids.map(id => [id, root.querySelector(`#${id}`)]));
  const rows = BOOK_FONT_SLOTS.map(slot => ({ slot,
    file: root.querySelector(`#book-font-file-${slot}`), weight: root.querySelector(`#book-font-weight-${slot}`),
    current: root.querySelector(`#book-font-current-${slot}`), remove: root.querySelector(`#book-font-remove-${slot}`),
  }));
  if (Object.values(el).some(node => !node) || rows.some(row => !row.file || !row.weight || !row.current || !row.remove)
      || typeof confirm !== "function") throw new TypeError("Book font controls and an explicit confirmation handler are required.");
  let disposed = false, suspended = false, operation = null, inventory = "";
  const unlisten = [], session = createBookFontAuthoring({ collection, controls, worker });
  const status = text => { if (!disposed) el["book-font-status"].textContent = text; };
  const drafts = () => rows.map(row => ({ file: row.file.files?.[0] ?? null,
    count: row.file.files?.length ?? 0, weight: row.weight.value }));
  const matches = captured => drafts().every((now, i) => now.file === captured[i].file
    && now.count === captured[i].count && now.weight === captured[i].weight);
  function clearDrafts() {
    const fonts = new Map(collection.fonts.map(font => [font.slot, font]));
    for (const row of rows) { row.file.value = ""; row.weight.value = String(fonts.get(row.slot)?.weight ?? ""); }
  }
  function refresh() {
    if (disposed) return;
    const fonts = collection.fonts, metadata = JSON.stringify(fonts);
    if (metadata !== inventory) { inventory = metadata; clearDrafts(); }
    const map = new Map(fonts.map(font => [font.slot, font]));
    const stopped = suspended || controls.sourceBusy, busy = session.busy || operation !== null;
    let selected = 0;
    for (const row of rows) {
      const font = map.get(row.slot);
      row.current.textContent = font ? `${font.name} — ${font.size.toLocaleString()} bytes — weight ${font.weight ?? "role default"}`
        : "No supplied font; normal engine fallback.";
      // Changing a draft can cancel a running operation instead of trapping the
      // user in an invalid choice. Silent draft edits are fenced by isCurrent.
      row.file.disabled = row.weight.disabled = stopped;
      row.remove.disabled = stopped || busy || !font;
      if (row.file.files?.length) selected++;
    }
    el["book-font-inventory"].textContent = `${fonts.length} supplied roles · ${fonts.reduce((n, font) => n + font.size, 0).toLocaleString()} bytes`;
    el["book-font-assign"].disabled = stopped || busy || !selected;
    el["book-font-cancel"].disabled = !busy;
    el["book-font-discard"].disabled = stopped || (!selected && !busy);
    el["book-font-clear"].disabled = stopped || busy || !fonts.length;
    el["book-font-panel"].setAttribute("aria-busy", String(busy));
    if (operation) {
      const message = { reading: "Reading selected font files; existing assignments are unchanged.",
        validating: "Validating proposed fonts through the native PDF worker; existing assignments are unchanged.",
        confirming: "Review font removal before confirming." }[session.phase];
      if (message) status(message);
    }
  }
  function retire(message) {
    const previous = operation; operation = null;
    previous?.abort.abort(); session.cancel(message);
    if (message) status(message);
    refresh();
  }
  async function run(work, completed) {
    if (disposed || suspended) throw failure("FONT_CLOSED", "Book font controls are not active.");
    if (operation || session.busy || controls.sourceBusy) throw failure("BOOK_BUSY", "Finish the current operation or text composition first.");
    // Capture pending source first: its synchronous notification must not be
    // mistaken for a change to the selection this operation is about to own.
    controls.captureProject();
    const job = { abort: new AbortController(), draft: drafts() }; operation = job;
    const options = { signal: job.abort.signal,
      isCurrent: () => !disposed && !suspended && operation === job && matches(job.draft) };
    try {
      const result = await work(options);
      if (operation === job && !disposed) {
        clearDrafts(); status(result ? completed : "Removal declined or no supplied font was present. Current assignments were kept.");
      }
      return result;
    } catch (error) {
      if (operation === job) status(`${error.code ?? "FONT_ERROR"}: ${error.message ?? "Font operation failed."} No pending font changes were installed.`);
      throw error;
    } finally { if (operation === job) operation = null; refresh(); }
  }
  async function assign() {
    const selected = [];
    for (const row of rows) {
      if (!row.file.files?.length) continue;
      if (row.file.files.length !== 1) throw failure("INVALID_FONT", "Choose exactly one font file for each role.");
      const raw = row.weight.value.trim();
      if (raw && (!/^\d{1,4}$/.test(raw) || Number(raw) < 1 || Number(raw) > 1000))
        throw failure("INVALID_FONT_WEIGHT", "Weight must be blank or an integer from 1 through 1000; no font was assigned.");
      selected.push({ slot: row.slot, file: row.file.files[0], weight: raw ? Number(raw) : undefined });
    }
    return run(options => session.assign(selected, options),
      "Selected fonts passed native preflight and were assigned together. Rebuild the book preview or PDF proof to check the full document. Use a portable backup to retain these font bytes.");
  }
  const remove = slot => run(options => session.remove(slot, confirm, options), "Supplied font removed. Rebuild preview or proof to review the normal fallback.");
  const clear = () => run(options => session.clear(confirm, options), "All supplied fonts revoked. Source and images were retained.");
  function invoke(work) {
    try { Promise.resolve(work()).catch(error => {
      if (!operation) status(`${error.code ?? "FONT_ERROR"}: ${error.message ?? "Font operation failed."}`);
    }); } catch (error) { status(`${error.code ?? "FONT_ERROR"}: ${error.message}`); }
  }
  function on(node, event, listener) {
    node.addEventListener(event, listener); unlisten.push(() => node.removeEventListener(event, listener));
  }
  for (const row of rows) {
    for (const node of [row.file, row.weight]) {
      on(node, "input", () => retire("Font selection changed. Validate the selected files when ready."));
      on(node, "change", () => retire("Font selection changed. Validate the selected files when ready."));
    }
    on(row.remove, "click", () => invoke(() => remove(row.slot)));
  }
  for (const id of ["chapter-source", "chapter-path", "source-role", "chapters", "title", "author", "lang", "font", "dark-mode", "font-scale", "toc", "page-numbers"]) {
    const node = root.querySelector(`#${id}`);
    if (node) for (const event of ["input", "change", "compositionstart"]) on(node, event, () => {
      if (operation) retire("The editor changed. No pending font changes were installed; retry when ready.");
    });
  }
  on(el["book-font-assign"], "click", () => invoke(assign));
  on(el["book-font-cancel"], "click", () => retire("Font operation cancelled; current assignments were kept."));
  on(el["book-font-discard"], "click", () => { retire(); clearDrafts(); refresh(); status("Selected files discarded. Installed fonts are unchanged."); });
  on(el["book-font-clear"], "click", () => invoke(clear));
  const unCollection = collection.subscribe(refresh), unSession = session.subscribe(refresh);
  refresh();
  return Object.freeze({
    assign, remove, clear,
    suspend() {
      if (disposed) return;
      suspended = true; retire("Font authoring suspended; pending files were released.");
      session.suspend(); clearDrafts(); refresh();
    },
    resume() { if (!disposed) { suspended = false; session.resume(); refresh(); status("Font authoring resumed. Reselect local files; no fonts were restored automatically."); } },
    dispose() {
      if (disposed) return;
      retire(); clearDrafts(); disposed = true;
      unCollection(); unSession(); for (const remove of unlisten) remove(); session.dispose();
    },
  });
}
