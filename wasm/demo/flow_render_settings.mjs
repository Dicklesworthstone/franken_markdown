// Applied renderer configuration, not CSS pretending to change engine metrics.
// Session-only primitives: never source, file handles, images or trust grants.

import { normalizeFlowExport } from "../flow_export.mjs";
import { FlowError, validateCreation } from "../flow_session.mjs";

const DEFAULTS = Object.freeze({
  font: "sans",
  bodySize: 14,
  codeSize: 13,
  lineHeight: 20,
  title: "",
  lang: "",
  toc: null,
  tocDepth: null,
  author: "",
  pageNumbers: true,
  codeLineNumbers: null,
  baseFontSize: null,
  headingScale: null,
  tableFontSize: null,
  fitToPages: null,
  microtype: null,
  darkMode: "auto",
});
const PRESETS = Object.freeze({
  standard: Object.freeze({ font: "sans", bodySize: 14, codeSize: 13, lineHeight: 20 }),
  reading: Object.freeze({ font: "serif", bodySize: 18, codeSize: 15, lineHeight: 27 }),
  compact: Object.freeze({ font: "sans", bodySize: 12, codeSize: 11, lineHeight: 17 }),
});
const fail = (message) => {
  throw new FlowError("INVALID_OPTIONS", message);
};
const common = ["title", "lang", "toc", "tocDepth"];
const pdfOnly = [
  "author",
  "pageNumbers",
  "codeLineNumbers",
  "baseFontSize",
  "headingScale",
  "tableFontSize",
  "fitToPages",
  "microtype",
];
const equal = (a, b) => Object.keys(DEFAULTS).every((key) => a.values[key] === b.values[key]);
const previewEqual = (a, b) =>
  Object.keys(a.preview).every((key) => a.preview[key] === b.preview[key]);

export function normalizeRenderSettings(input = {}) {
  if (
    !input ||
    typeof input !== "object" ||
    Array.isArray(input) ||
    Object.keys(input).some((key) => !Object.hasOwn(DEFAULTS, key))
  )
    fail("Unsupported rendering settings.");
  const values = Object.fromEntries(
    Object.entries(DEFAULTS).map(([key, value]) => [
      key,
      input[key] === undefined ? value : input[key],
    ]),
  );
  const { font, layout } = validateCreation("", {
    font: values.font,
    bodySize: values.bodySize,
    codeSize: values.codeSize,
    lineHeight: values.lineHeight,
  });
  for (const [key, min, max] of [
    ["bodySize", 8, 48],
    ["codeSize", 6, 40],
    ["lineHeight", 8, 72],
  ]) {
    if (layout[key] < min || layout[key] > max) fail(`${key} must be from ${min} to ${max}.`);
    values[key] = layout[key];
  }
  const preview = Object.freeze({
    font,
    bodySize: layout.bodySize,
    codeSize: layout.codeSize,
    lineHeight: layout.lineHeight,
  });
  for (const key of ["title", "lang", "author"])
    if (typeof values[key] !== "string")
      fail(`${key} must be text; use empty text for the renderer default.`);
  // Null is the explicit "renderer default" for optional switches/numbers. Do
  // not replace an omitted engine default with a guessed application default.
  for (const key of ["toc", "pageNumbers", "codeLineNumbers"]) {
    if (values[key] !== null && typeof values[key] !== "boolean")
      fail(`${key} must be boolean or null.`);
  }
  for (const key of ["tocDepth", "baseFontSize", "headingScale", "tableFontSize", "fitToPages"]) {
    if (values[key] !== null && typeof values[key] !== "number")
      fail(`${key} must be numeric or null.`);
  }
  if (![null, "disabled", "protrusion"].includes(values.microtype)) fail("Invalid microtype.");
  if (!["auto", "disabled"].includes(values.darkMode)) fail("Invalid darkMode.");
  const selected = (keys) =>
    Object.fromEntries(
      keys
        .filter((key) => values[key] !== null && values[key] !== "")
        .map((key) => [key, values[key]]),
    );
  // Validation only: real revision tokens are supplied by export admission.
  const validationToken = { revision: "0", layoutRevision: "0" };
  const [, html] = normalizeFlowExport(
    "html",
    { ...selected(common), darkMode: values.darkMode },
    validationToken,
  );
  const [, pdf] = normalizeFlowExport(
    "pdf",
    { ...selected([...common, ...pdfOnly]), metadataEpochSeconds: 0 },
    validationToken,
  );
  return Object.freeze({ values: Object.freeze(values), preview, html, pdf });
}

const booleanChoices = [
  ["", "Renderer default"],
  ["true", "On"],
  ["false", "Off"],
];
const FIELDS = [
  [
    "font",
    "Bundled font (preview and exports)",
    "select",
    [
      ["sans", "Sans"],
      ["serif", "Serif"],
    ],
  ],
  ["bodySize", "Canvas body size", "number", 8, 48],
  ["codeSize", "Canvas code size", "number", 6, 40],
  ["lineHeight", "Canvas line height", "number", 8, 72],
  ["title", "Export title (empty uses renderer default)", "text", 4096],
  ["lang", "Export language tag (for example, de-DE)", "text", 64],
  ["toc", "Export table of contents", "boolean", booleanChoices],
  ["tocDepth", "Contents depth (empty uses renderer default)", "number", 1, 6, true],
  ["author", "PDF author (empty uses renderer default)", "text", 4096],
  ["pageNumbers", "PDF page numbers", "boolean", booleanChoices],
  ["codeLineNumbers", "PDF code line numbers", "boolean", booleanChoices],
  ["baseFontSize", "PDF body size in points (optional)", "number", 6, 24],
  ["headingScale", "PDF heading scale (optional)", "number", 1.05, 2],
  ["tableFontSize", "PDF table size in points (optional)", "number", 5, 24],
  ["fitToPages", "PDF target page count (optional, not guaranteed)", "number", 1, 1000, true],
  [
    "microtype",
    "PDF optical margin alignment",
    "select",
    [
      ["", "Renderer default"],
      ["disabled", "Disabled"],
      ["protrusion", "Protrusion"],
    ],
  ],
  [
    "darkMode",
    "HTML appearance",
    "select",
    [
      ["auto", "Follow reader's light/dark preference"],
      ["disabled", "Light only"],
    ],
  ],
];
function mount(root) {
  let panel = root.querySelector("#render-settings");
  if (!panel) {
    const make = (tag, text, id) => {
      const node = root.createElement(tag);
      if (tag === "button") node.style.minHeight = "44px";
      if (text !== undefined) node.textContent = text;
      if (id) node.id = id;
      return node;
    };
    panel = make("section", undefined, "render-settings");
    panel.setAttribute("aria-labelledby", "render-settings-title");
    const title = make("h2", "Typography and publishing", "render-settings-title");
    const help = make(
      "p",
      "Apply changes to use the Rust engine's measured typography. Canvas sizes affect only the flowing preview; PDF sizes use points and its separate page layout. The bundled font applies to both exports. Settings do not modify or save Markdown, and do not enable raw HTML or network images.",
      "render-settings-help",
    );
    panel.append(title, help);
    const form = make("form", undefined, "render-settings-form");
    // Application validation preserves all draft field values and explains
    // cross-field constraints; it does not rely on browser number clamping.
    form.noValidate = true;
    let group;
    for (const [key, label, type, first, max, integer] of FIELDS) {
      const groupTitle =
        key === "font"
          ? "Measured preview"
          : key === "title"
            ? "Shared export options"
            : key === "author"
              ? "PDF publishing"
              : key === "darkMode"
                ? "HTML publishing"
                : null;
      if (groupTitle) {
        group = make("fieldset");
        group.append(make("legend", groupTitle));
        form.append(group);
      }
      const row = make("p"),
        caption = make("label", label),
        id = `render-${key}`;
      caption.setAttribute("for", id);
      const field = make(
        type === "select" || type === "boolean" ? "select" : "input",
        undefined,
        id,
      );
      field.setAttribute("aria-describedby", "render-settings-help render-settings-status");
      field.style.maxWidth = "100%";
      field.style.minHeight = "44px";
      if (field.tagName.toLowerCase() === "select") {
        for (const [value, text] of first) {
          const option = make("option", text);
          option.value = value;
          field.append(option);
        }
      } else {
        field.type = type;
        field.autocomplete = "off";
        if (type === "number") {
          field.min = String(first);
          field.max = String(max);
          field.step = integer ? "1" : "any";
        } else {
          field.maxLength = first;
          field.spellcheck = false;
        }
      }
      row.append(caption, field);
      group.append(row);
    }
    const presets = make("p");
    for (const [key, text] of [
      ["standard", "Standard preview"],
      ["reading", "Reading preview"],
      ["compact", "Compact preview"],
    ]) {
      const button = make("button", text, `render-preset-${key}`);
      button.type = "button";
      presets.append(button);
    }
    form.append(
      presets,
      make(
        "p",
        "Presets change only the preview draft fields, including the shared font. Click Apply to use them. Blank optional fields preserve renderer defaults; a target page count is a fit request, not a guarantee.",
      ),
    );
    const actions = make("p");
    for (const [id, text] of [
      ["apply", "Apply settings"],
      ["discard", "Discard unapplied changes"],
      ["reset", "Reset all settings"],
    ]) {
      const button = make("button", text, `render-settings-${id}`);
      button.type = id === "apply" ? "submit" : "button";
      actions.append(button);
    }
    const status = make("p", "", "render-settings-status");
    status.setAttribute("role", "status");
    status.style.overflowWrap = "anywhere";
    form.append(actions, status);
    panel.append(form);
    const guide = make("a", "Typography and publishing details");
    guide.href = "../SETTINGS.md";
    panel.append(guide);
    const anchor = root.querySelector("#export-controls");
    if (!anchor?.parentNode) fail("The export controls mount is missing.");
    anchor.parentNode.insertBefore(panel, anchor);
  }
  const fields = Object.fromEntries(
    FIELDS.map(([key]) => [key, root.querySelector(`#render-${key}`)]),
  );
  const elements = Object.fromEntries(
    ["form", "apply", "discard", "reset", "status"].map((key) => [
      key,
      root.querySelector(`#render-settings-${key}`),
    ]),
  );
  const presets = Object.fromEntries(
    Object.keys(PRESETS).map((key) => [key, root.querySelector(`#render-preset-${key}`)]),
  );
  if (
    [...Object.values(fields), ...Object.values(elements), ...Object.values(presets)].some(
      (value) => !value,
    )
  )
    fail("Rendering settings markup is incomplete.");
  return { fields, ...elements, presets };
}

export function createRenderSettingsControls({
  root,
  sourceEditor,
  onApply,
  onInvalidate = () => {},
  initial,
}) {
  if (typeof onApply !== "function" || typeof onInvalidate !== "function")
    fail("A synchronous rendering settings callback is required.");
  let applied = normalizeRenderSettings(initial),
    disposed = false,
    composing = false,
    applying = false;
  const ui = mount(root),
    listeners = [];
  for (const field of Object.values(ui.fields)) field.disabled = false;
  const alive = () => {
    if (disposed)
      throw new FlowError("SESSION_DISPOSED", "Rendering settings controls are disposed.");
  };
  const write = (value) => {
    for (const [key, field] of Object.entries(ui.fields))
      field.value = value.values[key] === null ? "" : String(value.values[key]);
  };
  function draft() {
    const values = {};
    for (const [key, , type, , , integer] of FIELDS) {
      const text = ui.fields[key].value;
      if (type === "number") {
        if (text === "" && DEFAULTS[key] === null) values[key] = null;
        else {
          if (!text.trim()) fail(`${key} is required.`);
          values[key] = Number(text);
          if (!Number.isFinite(values[key]) || (integer && !Number.isInteger(values[key])))
            fail(`Invalid ${key}.`);
        }
      } else if (type === "boolean") {
        if (!["", "true", "false"].includes(text)) fail(`Invalid ${key}.`);
        values[key] = text === "" ? null : text === "true";
      } else values[key] = key === "microtype" && text === "" ? null : text;
    }
    return normalizeRenderSettings(values);
  }
  function pending() {
    try {
      return !equal(draft(), applied);
    } catch {
      return true;
    }
  }
  function refresh(message) {
    const dirty = pending();
    ui.apply.disabled = disposed || composing || !dirty;
    ui.discard.disabled = disposed || composing || !dirty;
    ui.reset.disabled = disposed || composing;
    for (const button of Object.values(ui.presets)) button.disabled = disposed || composing;
    if (!disposed)
      ui.status.textContent =
        message ??
        (dirty
          ? "Unapplied settings. Apply or discard them before preparing an export. Current Markdown is unchanged."
          : "Settings applied. The preview reports engine progress separately; no file has been saved. Publishing options reset when a different document is opened or restored.");
  }
  function commit(next) {
    alive();
    if (composing || applying)
      throw new FlowError(
        "SETTINGS_BUSY",
        "Finish text composition or the current settings operation first.",
      );
    if (equal(next, applied)) {
      write(applied);
      refresh();
      return false;
    }
    applying = true;
    try {
      // The host invalidates exports synchronously and submits the preview
      // intent using next.preview. No asynchronous renderer success is implied.
      onApply(next, Object.freeze({ previewChanged: !previewEqual(next, applied) }));
      applied = next;
      write(applied);
      refresh();
      return true;
    } finally {
      applying = false;
    }
  }
  const apply = () => commit(draft());
  const reset = () => commit(normalizeRenderSettings());
  const listen = (element, type, handler) => {
    element.addEventListener(type, handler);
    listeners.push(() => element.removeEventListener(type, handler));
  };
  const run = (action) => (event) => {
    if (disposed || event?.defaultPrevented) return;
    event?.preventDefault();
    try {
      action();
    } catch (error) {
      refresh(
        `${error instanceof FlowError ? `${error.code}: ${error.message}` : "Settings could not be applied."} Previous applied settings and source were retained.`,
      );
    }
  };
  listen(ui.form, "submit", run(apply));
  const changed = () => {
    onInvalidate();
    refresh();
  };
  listen(ui.form, "input", changed);
  listen(ui.form, "change", changed);
  listen(ui.form, "compositionstart", () => {
    composing = true;
    refresh();
  });
  listen(ui.form, "compositionend", () => {
    composing = false;
    refresh();
  });
  listen(
    ui.discard,
    "click",
    run(() => {
      write(applied);
      refresh();
    }),
  );
  listen(ui.reset, "click", run(reset));
  for (const [key, button] of Object.entries(ui.presets))
    listen(
      button,
      "click",
      run(() => {
        if (composing) return;
        for (const [name, value] of Object.entries(PRESETS[key]))
          ui.fields[name].value = String(value);
        onInvalidate();
        refresh(
          "Preview preset loaded as unapplied fields. Apply to use it; publishing fields were retained.",
        );
      }),
    );
  listen(
    sourceEditor,
    "fmd-document-replaced",
    run(() => {
      // A new file must not inherit another document's author/title/language.
      composing = false;
      commit(normalizeRenderSettings(applied.preview));
    }),
  );
  write(applied);
  refresh();
  return Object.freeze({
    get settings() {
      alive();
      return applied;
    },
    get dirty() {
      alive();
      return pending();
    },
    apply,
    reset,
    exportOptions(format) {
      alive();
      if (!["html", "pdf"].includes(format)) fail("Choose HTML or PDF export.");
      if (pending())
        throw new FlowError(
          "UNAPPLIED_SETTINGS",
          "Apply or discard the settings fields before exporting.",
        );
      return applied[format];
    },
    dispose() {
      if (disposed) return;
      disposed = true;
      for (const remove of listeners) remove();
      listeners.length = 0;
      for (const field of Object.values(ui.fields)) field.disabled = true;
      refresh();
      applied = normalizeRenderSettings();
    },
  });
}
