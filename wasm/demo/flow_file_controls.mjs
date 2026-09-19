import { FlowError } from "../flow_session.mjs";
import { documentName } from "./flow_document.mjs";
import { createFileDocumentSession } from "./flow_file_session.mjs";

const buttons = Object.freeze({
  "file-open": "Open editable Markdown", "file-save": "Save current file",
  "file-save-as": "Save Markdown as…", "file-reload": "Reload file version",
  "file-keep": "Keep editing", "file-backup": "Prepare recovery download",
  "file-disconnect": "Disconnect file"
});
function mount(root) {
  let panel = root.querySelector("#file-controls");
  if (!panel) {
    panel = root.createElement("fieldset"); panel.id = "file-controls";
    const legend = root.createElement("legend"); legend.textContent = "Direct Markdown file saving";
    const help = root.createElement("p"); help.id = "file-help";
    help.textContent = "These optional controls can overwrite a file you explicitly choose. No automatic disk saves occur. Open/reload replaces source after confirmation; saving preserves edits and image access. Handles are never stored and are disconnected when this page is suspended.";
    panel.append(legend, help);
    const actions = root.createElement("p");
    for (const [id, label] of Object.entries(buttons)) {
      const button = root.createElement("button"); button.id = id; button.type = "button";
      button.textContent = label; button.disabled = true;
      button.setAttribute("aria-describedby", "file-help file-status");
      button.style.minHeight = "44px"; button.style.margin = "4px";
      actions.append(button);
    }
    const status = root.createElement("p"); status.id = "file-status"; status.setAttribute("role", "status");
    status.style.overflowWrap = "anywhere";
    const caution = root.createElement("p");
    caution.textContent = "A saved baseline is not continuous disk monitoring. Save checks exact contents before overwrite and verifies them afterward, but another application can still race the final close. A conflict never auto-reloads or force-overwrites. Keep editing, save to a different file, or prepare a download before reloading.";
    const guide = root.createElement("a"); guide.href = "../FILES.md"; guide.textContent = "File saving details and browser limits";
    panel.append(actions, status, caution, guide);
    const anchor = root.querySelector("#source-status");
    if (!anchor?.parentNode) throw new Error("Source status mount is missing.");
    anchor.parentNode.insertBefore(panel, anchor.nextSibling);
  }
  const elements = Object.fromEntries([...Object.keys(buttons), "file-status"].map(id => [id, root.querySelector(`#${id}`)]));
  if (Object.values(elements).some(value => !value)) throw new Error("File controls markup is incomplete.");
  return { panel, elements };
}

/** Independent of renderer and storage. Callbacks into the existing source
 * controller preserve its original bytes, replacement events and downloads.
 * Native file/permission dialogs must remain in the direct click/key task.
 */
export function createFileControls({ root, window, controls, sourceEditor, filename,
  confirm = text => window.confirm(text) }) {
  const { elements: el } = mount(root), listeners = [];
  const canOpen = window.isSecureContext === true && typeof window.showOpenFilePicker === "function";
  const canChoose = window.isSecureContext === true && typeof window.showSaveFilePicker === "function";
  const ordinaryOpen = root.querySelector("#open-markdown");
  let disposed = false, composing = false, touched = false, unloadListening = false, session;
  const types = [{ description: "UTF-8 Markdown", accept: { "text/plain": [".md", ".markdown", ".txt"] } }];
  const beforeUnload = event => {
    const state = session.state;
    if (touched && (state.dirty || state.busy || ["conflict", "uncertain"].includes(state.phase))) {
      event.preventDefault(); event.returnValue = "";
    }
  };
  function refresh(state = session.state) {
    if (disposed) return;
    const busy = !!state.busy || composing || sourceEditor.disabled || sourceEditor.readOnly;
    el["file-open"].disabled = busy || !canOpen;
    el["file-save"].disabled = busy || !state.canSave;
    el["file-save-as"].disabled = busy || !canChoose || !state.valid;
    el["file-reload"].disabled = busy || !state.filename;
    el["file-disconnect"].disabled = !state.filename && !state.busy;
    el["file-backup"].disabled = composing || !state.valid;
    el["file-keep"].disabled = composing;
    const phases = {
      unlinked: "No connected file; source has not been saved through these controls.",
      saved: "Current source matches the last verified disk baseline. External changes are checked when saving, not monitored continuously.",
      edited: "Edited: current source differs from the last verified disk baseline.",
      renamed: "Source filename changed. Use Save Markdown as to choose the intended target.",
      conflict: "Disk conflict: current-file saving is paused. Keep editing, save to a different file, or reload explicitly.",
      uncertain: "Save outcome uncertain: do not assume this file was saved. Keep a separate copy before reloading.",
      invalid: "Repair the source or filename before saving. Invalid input has not been discarded."
    };
    const availability = !canOpen && !canChoose
      ? "Direct file pickers are unavailable here. The ordinary file input and Prepare Markdown download still work. " : "";
    el["file-status"].textContent = `${availability}${state.filename ? `Connected file: ${state.filename}. ` : ""}${phases[state.phase] ?? ""} ${state.message}`;
    const warn = touched && (state.dirty || !!state.busy || ["conflict", "uncertain"].includes(state.phase));
    if (warn && !unloadListening) { window.addEventListener("beforeunload", beforeUnload); unloadListening = true; }
    else if (!warn && unloadListening) { window.removeEventListener("beforeunload", beforeUnload); unloadListening = false; }
  }
  session = createFileDocumentSession({
    readDocument: () => controls.snapshot(), replaceDocument: next => controls.replace(next),
    renameDocument: name => {
      filename.value = name; filename.dispatchEvent(new Event("input", { bubbles: true }));
    },
    isBlocked: () => composing || sourceEditor.disabled || sourceEditor.readOnly || !!ordinaryOpen?.disabled,
    onState: refresh
  });
  const confirmation = ({ filename: name }) => confirm(`Replace the editor with ${name}? Current unsaved edits and undo history will be replaced, and image access revoked. Prepare a recovery download first to keep a separate copy.`);
  function open() {
    return session.open(canOpen ? () => window.showOpenFilePicker({ multiple: false, excludeAcceptAllOption: true, types }) : undefined, confirmation);
  }
  function saveAs() {
    return session.saveAs(canChoose ? name => {
      const suggestedName = documentName(/\.(md|markdown|txt)$/i.test(name) ? name : `${name}.md`);
      return window.showSaveFilePicker({ suggestedName, excludeAcceptAllOption: true, types });
    } : undefined, ({ filename: name }) => confirm(`Overwrite the existing contents of ${name} with this Markdown source? This changes the selected file on disk.`));
  }
  function backup() {
    if (disposed) throw new FlowError("SESSION_DISPOSED", "File controls are disposed.");
    controls.prepareDownload();
    root.querySelector("#source-download")?.focus();
    el["file-status"].textContent = "Recovery Markdown prepared. Click the source Download link to save it. This has not overwritten a file or marked the current file saved.";
  }
  function save() {
    const state = session.state;
    if (state.filename && state.phase !== "renamed") return session.save();
    return canChoose ? saveAs() : backup();
  }
  function report(error) {
    if (disposed) return;
    refresh();
    el["file-status"].textContent += ` ${error instanceof FlowError ? `${error.code}: ${error.message}` : "File controls failed; prepare a source download to keep your work."}`;
  }
  const listen = (element, type, handler) => {
    element.addEventListener(type, handler); listeners.push(() => element.removeEventListener(type, handler));
  };
  // Invoke action before wrapping its result: deferring it to a microtask would
  // needlessly risk the user activation required by native permission prompts.
  const run = action => event => {
    if (disposed || event?.defaultPrevented || event?.currentTarget?.disabled) return;
    try { Promise.resolve(action()).catch(report); } catch (error) { report(error); }
  };
  const changed = () => { touched = true; session.changed(); };
  listen(sourceEditor, "input", changed); listen(filename, "input", changed);
  listen(sourceEditor, "fmd-document-replaced", () => session.detach());
  listen(sourceEditor, "compositionstart", () => { composing = true; changed(); });
  listen(sourceEditor, "compositionend", () => { composing = false; changed(); });
  if (ordinaryOpen) listen(ordinaryOpen, "change", () => session.changed());
  listen(el["file-open"], "click", run(open));
  listen(el["file-save"], "click", run(save));
  listen(el["file-save-as"], "click", run(saveAs));
  listen(el["file-reload"], "click", run(() => session.reload(confirmation)));
  listen(el["file-disconnect"], "click", run(() => session.detach()));
  listen(el["file-backup"], "click", run(backup));
  listen(el["file-keep"], "click", run(() => sourceEditor.focus()));
  listen(window, "keydown", event => {
    if (disposed || event.defaultPrevented || event.altKey || !(event.ctrlKey || event.metaKey)
        || event.key?.toLowerCase() !== "s" || event.isComposing || event.keyCode === 229 || composing) return;
    event.preventDefault();
    if (event.repeat) return;
    run(event.shiftKey ? () => canChoose ? saveAs() : backup() : save)();
  });
  listen(window, "focus", () => refresh());
  refresh();
  return Object.freeze({ open, save, saveAs, reload: () => session.reload(confirmation),
    backup, disconnect: () => session.detach(), get state() { return session.state; },
    dispose() {
      if (disposed) return;
      disposed = true; session.dispose();
      for (const remove of listeners) remove(); listeners.length = 0;
      if (unloadListening) window.removeEventListener("beforeunload", beforeUnload);
      unloadListening = false;
      for (const id of Object.keys(buttons)) el[id].disabled = true;
      el["file-status"].textContent = "File access disconnected. Source is retained; reopen explicitly after returning to the page.";
    }
  });
}
