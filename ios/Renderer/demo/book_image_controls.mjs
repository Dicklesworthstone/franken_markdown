import { BOOK_WORKBENCH_LIMITS, readBookFiles } from "./book_collection.mjs";
import { bookError } from "../book_worker.mjs";

const IDS = ["book-image-selection", "book-image-file", "book-image-replace", "book-image-remove", "book-image-cancel", "book-image-status"];
const format = name => /\.png$/i.test(name) ? "png" : /\.jpe?g$/i.test(name) ? "jpeg" : /\.svg$/i.test(name) ? "svg" : null;

/** Byte authorization uses the ordinary image-import policy. This controller
 * never parses Markdown, loads an image into the host DOM or claims that an
 * admitted file is supported by every output renderer. */
export function createBookImagePanel(root) {
  if (root.querySelector("#book-image-panel")) return;
  const section = root.querySelector("#arrange-title")?.parentElement;
  if (!section) throw new Error("Missing book arrangement section.");
  const make = (tag, id, text, parent) => {
    const node = root.createElement(tag);
    if (id) node.id = id;
    if (text) node.textContent = text;
    parent?.appendChild(node);
    return node;
  };
  const panel = make("div", "book-image-panel", "", section);
  make("h3", "book-image-title", "Replace or remove book images", panel);
  make("p", "book-image-help", "Replace bytes at an existing book-relative image path without editing Markdown references or alt text. Select a PNG, JPEG or SVG of the same format. File admission uses the ordinary import limits; rebuild the book preview or PDF proof to check rendering. No file is uploaded or changed on disk.", panel);
  const label = make("label", "", "Authorized image path", panel);
  label.setAttribute("for", "book-image-selection");
  const select = make("select", IDS[0], "", panel);
  select.size = 5;
  select.setAttribute("aria-describedby", "book-image-help book-image-status");
  const fileLabel = make("label", "", "Replacement image (at most 8 MiB)", panel);
  fileLabel.setAttribute("for", "book-image-file");
  const file = make("input", IDS[1], "", panel);
  file.type = "file";
  file.accept = ".png,.jpg,.jpeg,.svg,image/png,image/jpeg,image/svg+xml";
  file.setAttribute("aria-describedby", "book-image-help book-image-status");
  for (const [id, text] of [[IDS[2], "Replace selected image"], [IDS[3], "Remove selected image"], [IDS[4], "Cancel image operation"]]) {
    const button = make("button", id, text, panel);
    button.type = "button";
  }
  make("p", "", "Removal does not rewrite references or infer whether an image is unused. Remaining references may show missing-image fallbacks. Source undo does not restore resources; prepare a portable backup before removing images you may need again.", panel);
  const status = make("p", IDS[5], "Select an authorized image to manage.", panel);
  status.setAttribute("role", "status");
  status.setAttribute("aria-live", "polite");
}

function wait(value, signal) {
  return new Promise((resolve, reject) => {
    const clean = () => signal.removeEventListener("abort", abort);
    const abort = () => { clean(); reject(signal.reason); };
    signal.addEventListener("abort", abort, { once: true });
    // Attach both handlers even if cancellation happened during the read call.
    Promise.resolve(value).then(result => { clean(); resolve(result); }, error => { clean(); reject(error); });
    if (signal.aborted) abort();
  });
}

export function createBookImageControls({ root, collection, controls, confirm, timeoutMs = 30000 }) {
  const el = Object.fromEntries(IDS.map(id => [id, root.querySelector(`#${id}`)]));
  if (Object.values(el).some(node => !node) || typeof collection?.changeImages !== "function"
      || typeof controls?.captureProject !== "function" || typeof controls?.checkpoint !== "function"
      || typeof controls?.subscribeSourceState !== "function") throw new TypeError("Missing book image controls or collection API.");
  if (!Number.isInteger(timeoutMs) || timeoutMs < 1 || timeoutMs > 120000)
    throw new TypeError("Invalid image operation timeout.");
  const select = el[IDS[0]], file = el[IDS[1]], status = el[IDS[5]], unlisten = [];
  let pending = null, suspended = false, disposed = false, inventory = "";
  let configuration = collection.renderConfigurationRevision;
  function refresh() {
    if (disposed) return;
    const images = collection.images, signature = JSON.stringify(images), previous = select.value;
    if (inventory !== signature) {
      select.replaceChildren(...images.map(image => {
        const option = root.createElement("option");
        option.value = image.destination;
        option.textContent = `${image.destination} (${image.size.toLocaleString()} bytes)`;
        return option;
      }));
      select.value = images.some(image => image.destination === previous) ? previous : images[0]?.destination ?? "";
      inventory = signature;
    }
    const blocked = suspended || controls.sourceBusy;
    const selected = images.some(image => image.destination === select.value);
    select.disabled = file.disabled = blocked || pending !== null || !images.length;
    el[IDS[2]].disabled = blocked || pending !== null || !selected || file.files?.length !== 1;
    el[IDS[3]].disabled = blocked || pending !== null || !selected;
    el[IDS[4]].disabled = pending === null;
  }
  function cancel(message = "Image operation cancelled; committed resources are unchanged.") {
    const op = pending;
    if (!op || op.committing) return;
    pending = null;
    clearTimeout(op.timer);
    op.controller.abort(bookError("IMAGE_CANCELLED", message));
    if (!disposed) { status.textContent = message; refresh(); }
  }
  function ready() {
    if (disposed || suspended) throw bookError("SESSION_DISPOSED", "Book image controls are not active.");
    if (controls.sourceBusy) throw bookError("BOOK_BUSY", "Finish importing or composing text before changing images.");
  }
  function check(op) {
    ready();
    if (pending !== op || op.controller.signal.aborted) throw bookError("IMAGE_CANCELLED", "Image operation was retired.");
    if (collection.revision !== op.revision || controls.checkpoint() !== op.checkpoint
        || select.value !== op.destination || file.files?.[0] !== op.file)
      throw bookError("STALE_SOURCE", "The book or image selection changed; nothing was installed.");
  }
  async function run(kind) {
    ready();
    if (pending) throw bookError("BOOK_BUSY", "An image operation is already in progress.");
    const op = { destination: select.value, file: file.files?.[0], controller: new AbortController(), capturing: true, committing: false };
    pending = op;
    try {
      // Capture pending editor text using its existing lossless owner. Reserve
      // the operation before observers can reenter during that synchronous call.
      controls.captureProject();
      op.capturing = false;
      op.revision = collection.revision;
      op.checkpoint = controls.checkpoint();
      check(op);
      const images = collection.images, current = images.find(image => image.destination === op.destination);
      if (!current) throw bookError("IMAGE_NOT_FOUND", "Select an authorized image first.");
      op.timer = setTimeout(() => {
        if (pending === op) cancel("Image operation timed out; committed resources are unchanged.");
      }, timeoutMs);
      refresh();
      let change;
      if (kind === "remove") {
        if (typeof confirm !== "function") throw bookError("RESOURCE_AUTHORIZATION_REQUIRED", "Image removal needs an explicit confirmation handler.");
        status.textContent = "Review image removal. Source references will not be rewritten.";
        const approved = await wait(confirm(`Remove the image bound to ${op.destination}? Markdown references remain. Save a portable backup first; source undo cannot restore image bytes.`), op.controller.signal);
        check(op);
        if (approved !== true) { status.textContent = "Removal declined. The image is unchanged."; return false; }
        change = { destination: op.destination, remove: true };
      } else {
        if (file.files?.length !== 1 || !op.file || !format(op.file.name) || format(op.file.name) !== format(op.destination))
          throw bookError("INVALID_IMAGE", "Select one replacement PNG, JPEG or SVG of the same format as the image path.");
        const size = op.file.size;
        if (!Number.isSafeInteger(size) || size < 1 || size > BOOK_WORKBENCH_LIMITS.imageBytes)
          throw bookError("FILE_LIMIT", "Choose a replacement containing 1 byte through 8 MiB.");
        const total = images.reduce((sum, image) => sum + image.size, 0) - current.size + size;
        if (total > BOOK_WORKBENCH_LIMITS.totalImageBytes)
          throw bookError("BOOK_LIMIT", "The final image set would exceed 32 MiB; remove an unneeded image first.");
        status.textContent = "Reading replacement image. No committed resource has changed.";
        const result = await wait(readBookFiles([op.file]), op.controller.signal);
        check(op);
        if (result.images.length !== 1 || result.chapters.length || result.ignored)
          throw bookError("INVALID_IMAGE", "The selected file was not admitted as one image.");
        change = { destination: op.destination, bytes: result.images[0].bytes };
      }
      check(op);
      op.committing = true;
      const installed = collection.changeImages([change], op.revision);
      // From this point the synchronous commit succeeded. A subscriber may
      // cancel/modify/dispose the view, but that cannot turn success into rollback.
      if (!disposed && pending === op) {
        file.value = "";
        status.textContent = installed === op.revision
          ? "Replacement bytes are identical. Resources, previews and proofs are unchanged."
          : kind === "remove" ? "Image removed. Source references remain; rebuild the preview or PDF proof."
          : "Image replaced at the same path. Rebuild the preview or PDF proof to check rendering; portable backups retain the new bytes.";
      }
      return installed;
    } catch (error) {
      if (!disposed && pending === op)
        status.textContent = `${error?.code ?? "IMAGE_ERROR"}: ${String(error?.message ?? "Image operation failed.").slice(0, 1024)}`;
      throw error;
    } finally {
      clearTimeout(op.timer);
      if (pending === op) pending = null;
      refresh();
    }
  }
  function changed() {
    if (disposed || pending?.capturing) return;
    if (pending && !pending.committing) cancel("Book changed; image operation cancelled.");
    const next = collection.renderConfigurationRevision;
    if (next !== configuration) { configuration = next; file.value = ""; }
    refresh();
  }
  const unsubscribe = collection.subscribe(changed);
  const unsubscribeSource = controls.subscribeSourceState(() => {
    if (controls.sourceBusy) cancel("Import or composition started; image operation cancelled.");
    refresh();
  });
  const on = (node, type, listener) => {
    if (!node) return;
    node.addEventListener(type, listener);
    unlisten.push(() => node.removeEventListener(type, listener));
  };
  on(select, "change", () => { cancel("Image selection changed; old operation cancelled."); file.value = ""; refresh(); });
  on(file, "change", () => { cancel("Replacement selection changed; old operation cancelled."); refresh(); });
  on(el[IDS[2]], "click", () => { void run("replace").catch(() => {}); });
  on(el[IDS[3]], "click", () => { void run("remove").catch(() => {}); });
  on(el[IDS[4]], "click", () => cancel());
  // Invalid raw drafts may emit an event without a collection revision change.
  for (const id of ["chapter-source", "chapter-path", "chapters", "source-role", "title", "author", "lang", "font", "dark-mode", "font-scale", "toc", "page-numbers"]) {
    for (const type of ["input", "change"]) on(root.querySelector(`#${id}`), type, () => cancel("Editor changed; image operation cancelled."));
  }
  refresh();
  return Object.freeze({
    replace: () => run("replace"), remove: () => run("remove"), cancel,
    get pending() { return pending !== null; },
    suspend() { if (!disposed) { suspended = true; cancel(); file.value = ""; refresh(); } },
    resume() { if (!disposed) { suspended = false; refresh(); } },
    dispose() {
      if (disposed) return;
      disposed = true; cancel(); file.value = "";
      unsubscribe(); unsubscribeSource();
      for (const off of unlisten) off();
    },
  });
}
