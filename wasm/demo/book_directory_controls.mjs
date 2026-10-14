import { BookDirectoryError, createBookDirectorySession } from "./book_directory_session.mjs";

/** Source-folder saves use the existing publisher's capture/checkpoint route.
 * This controller never opens a project, installs disk source, acquires images
 * or fonts, starts a renderer, or owns the connected source-project JSON file.
 */
export function createBookDirectoryControls({ root, controls, collection, window }) {
  if (![controls?.captureProject, controls?.checkpoint, collection?.subscribe].every(fn => typeof fn === "function")
      || !Number.isSafeInteger(collection?.sourceProjectRevision))
    throw new TypeError("Source folders require a source host with project identity tracking.");
  const section = root.querySelector("#publish-title")?.parentElement;
  if (!section || root.querySelector("#book-directory")) throw new Error("Missing or duplicate source-folder panel.");
  const panel = root.createElement("div"); panel.id = "book-directory";
  panel.setAttribute("aria-labelledby", "book-directory-title");
  const make = (tag, id, text) => {
    const node = root.createElement(tag); node.id = `book-directory-${id}`;
    if (text !== undefined) node.textContent = text;
    panel.appendChild(node); return node;
  };
  make("h3", "title", "Connected source folder");
  make("p", "help", "Write chapters and include-only sources back to their relative paths in a folder you choose. First import or author the sources in this book, then review the folder. Existing differing files are overwrite candidates, not automatic imports. Connecting never writes. Save lists every create/replace before approval. Only source bytes are saved: keep a source-project file for chapter order, roles and publishing settings, or a portable backup for images and fonts.");
  const buttons = Object.fromEntries([["review", "Review / reconnect source folder"], ["save", "Save source files to folder"],
    ["disconnect", "Disconnect / stop folder access"], ["approve", "Approve reviewed folder"], ["decline", "Cancel review"]].map(([id, text]) => {
    const node = make("button", id, text); node.type = "button";
    node.setAttribute("aria-describedby", "book-directory-help book-directory-status"); return [id, node];
  }));
  const status = make("p", "status", ""); status.setAttribute("role", "status"); status.setAttribute("aria-live", "polite");
  const reviewText = make("pre", "review-list", "No folder review is pending.");
  reviewText.setAttribute("aria-label", "Exact source-folder review paths and actions");
  buttons.approve.setAttribute("aria-describedby", "book-directory-review-list book-directory-limits");
  const receiptText = make("pre", "receipt", "No source-folder save receipt yet.");
  receiptText.setAttribute("aria-label", "Last source-folder save receipt");
  make("p", "limits", "Each file is staged and read back, but multiple closes are NOT an atomic save or an OS-wide lock. An external editor can race the final checks. On failure, inspect the receipt: some files may be verified, uncertain, or created but unverified. Empty new files/directories can remain. Nothing is rolled back or deleted. Renaming/removing a source never deletes its old disk file. Path changes, conflicts and uncertain saves need explicit folder review. Limits: 256 sources, 4 MiB each, 16 MiB combined source/path bytes. Handles are session-only and disconnected on project replacement or page suspension.");
  section.appendChild(panel);
  let disposed = false, suspended = false, pending = null, session = null;
  let projectEpoch = collection.sourceProjectRevision;
  const listeners = [], composing = new Set();
  const sourceFields = new Set(["chapter-source", "chapter-path", "source-role", "chapters"]);
  const available = typeof window?.showDirectoryPicker === "function";
  const blocked = () => disposed || suspended || composing.size > 0 || controls.sourceBusy;
  function finish(approved, restoreFocus = false) {
    if (!pending) return;
    const previous = pending; pending = null;
    previous.resolve(approved);
    if (restoreFocus && previous.focus?.isConnected !== false) previous.focus?.focus?.();
    refresh();
  }
  function refresh(value = session?.state) {
    if (disposed || !value) return;
    const disabled = blocked();
    buttons.review.disabled = disabled || !!value.busy || !available || !value.files;
    buttons.save.disabled = disabled || !value.canSave;
    buttons.disconnect.disabled = suspended || (!value.folder && !value.busy);
    for (const name of ["approve", "decline"]) buttons[name].hidden = !pending;
    buttons.approve.disabled = disabled || !pending;
    buttons.decline.disabled = !pending;
    status.textContent = pending
      ? `${pending.kind === "bind" ? "Folder connection" : "Source-file save"} awaits approval. Review every listed create/replace; Cancel leaves disk content unchanged.`
      : `${value.folder ?? "No connected folder"} — ${value.phase}${value.busy ? ` (${value.busy})` : ""}. ${value.message}`;
    if (!available) status.textContent += " Direct folder access is unavailable here; ordinary imports, project downloads and portable backups remain available.";
    if (value.phase === "needs-review") status.textContent += " Use Review / reconnect source folder to admit the current path set explicitly.";
    const receipt = value.receipt;
    if (receipt) receiptText.textContent = [
      `Last operation: ${receipt.outcome}; ${receipt.verified} file(s) verified; ${receipt.current ? "captured editor state was current" : "not proof that current editor changes are saved"}.`,
      ...receipt.files.map(file => `${file.status}: ${file.path}`),
      ...receipt.createdDirectories.map(path => `Directory creation attempted (may remain): ${path}`),
      "No rollback or deletion was attempted. Keep this receipt when resolving a partial/uncertain save.",
    ].join("\n");
    else receiptText.textContent = "No source-folder save receipt yet.";
  }
  session = createBookDirectorySession({
    read: () => ({ files: collection.files, stamp: JSON.stringify([collection.sourceProjectRevision, controls.checkpoint()]) }),
    isBlocked: blocked, onState: refresh,
  });
  function admitted() {
    if (disposed || suspended) throw new BookDirectoryError("SESSION_DISPOSED", "Source-folder access is not active.");
    if (blocked() || session.state.busy) throw new BookDirectoryError("DIRECTORY_BUSY", "Finish importing, text composition or the current folder operation first.");
    // Reuse normal source/project admission; do not capture textarea drafts by
    // bypassing their host, and keep the picker inside the initiating gesture.
    controls.captureProject();
    if (blocked()) throw new BookDirectoryError("DIRECTORY_BUSY", "The source host became busy while capturing input.");
  }
  function confirm(kind, value) {
    if (blocked()) return Promise.resolve(false);
    if (pending) throw new BookDirectoryError("DIRECTORY_BUSY", "Another folder review is pending.");
    reviewText.textContent = [
      `${kind === "bind" ? "Connect" : "Write to"} folder: ${value.folder}`,
      ...value.files.map(file => `${file.action}: ${file.path} (${file.bytes} outgoing UTF-8 bytes)`),
      kind === "bind" ? "Approval connects these exact paths without writing any file. A later Save asks again before writing."
        : "Approval writes every listed create/replace from the captured book. Unchanged files and all unlisted files are left alone. Multiple files are not committed atomically.",
      "Source files only: no project metadata, chapter order, roles, images, fonts or permissions are saved.",
    ].join("\n");
    buttons.approve.textContent = kind === "bind" ? "Connect reviewed source paths" : "Write reviewed source files";
    return new Promise(resolve => {
      pending = { kind, resolve, focus: root.activeElement };
      refresh(); buttons.approve.focus?.();
    });
  }
  function bind() { admitted(); return session.bind(available ? () => window.showDirectoryPicker({ mode: "read" }) : null, value => confirm("bind", value)); }
  function save() { admitted(); return session.save(value => confirm("save", value)); }
  function detach() { if (!disposed) { finish(false); session.detach(); reviewText.textContent = "Folder review cancelled; no source was imported or replaced."; } }
  function invoke(work) {
    const report = error => { if (!disposed && !suspended) { refresh(); status.textContent += ` ${error.code ?? "DIRECTORY_ERROR"}: ${error.message}`; } };
    try { Promise.resolve(work()).catch(report); } catch (error) { report(error); }
  }
  function on(node, type, listener) {
    node?.addEventListener(type, listener); listeners.push(() => node?.removeEventListener(type, listener));
  }
  on(buttons.review, "click", () => invoke(bind));
  on(buttons.save, "click", () => invoke(save));
  on(buttons.disconnect, "click", detach);
  on(buttons.approve, "click", () => { if (!blocked()) finish(true, true); });
  on(buttons.decline, "click", () => finish(false, true));
  on(root, "keydown", event => {
    if (pending && !event.defaultPrevented && event.key === "Escape") { event.preventDefault(); finish(false, true); }
  });
  for (const type of ["input", "change"]) on(root, type, event => {
    if (!disposed && !suspended && sourceFields.has(event.target?.id)) { finish(false); session.changed(); }
  });
  on(root, "compositionstart", event => {
    if (sourceFields.has(event.target?.id)) { composing.add(event.target); finish(false); session.changed(); refresh(); }
  });
  on(root, "compositionend", event => { composing.delete(event.target); refresh(); });
  on(window, "beforeunload", event => {
    if (disposed || suspended) return;
    const value = session.state;
    if (value.busy || (value.folder && (value.dirty !== 0 || ["conflict", "uncertain", "needs-review", "invalid"].includes(value.phase)))) {
      event.preventDefault(); event.returnValue = "";
    }
  });
  const unsubscribe = collection.subscribe(() => {
    if (disposed || suspended) return;
    finish(false);
    const epoch = collection.sourceProjectRevision;
    if (epoch !== projectEpoch) { projectEpoch = epoch; detach(); }
    else session.changed();
  });
  const unsubscribeBusy = controls.subscribeSourceState?.(() => {
    if (disposed || suspended) return;
    if (controls.sourceBusy) { finish(false); session.changed(); }
    refresh();
  });
  refresh();
  return Object.freeze({ bind, save, detach, get state() { return session.state; },
    suspend() { if (!disposed) { suspended = true; composing.clear(); detach(); refresh(); } },
    resume() { if (!disposed) { suspended = false; projectEpoch = collection.sourceProjectRevision; refresh(); } },
    dispose() {
      if (disposed) return;
      finish(false); disposed = true; unsubscribe(); unsubscribeBusy?.();
      for (const remove of listeners) remove(); session.dispose(); composing.clear();
      for (const button of Object.values(buttons)) button.disabled = true;
    },
  });
}
