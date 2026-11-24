// Source-project I/O shares the original-source owner. It never starts a
// renderer, reads an image, or grants access to a file mentioned in Markdown.
import { FlowError } from "../flow_session.mjs";
import { trackSourceAccess } from "./flow_document.mjs";
import { captureFlowProject, flowProjectDownload, readFlowProjectFile } from "./flow_project.mjs";

export function createFlowProjectControls({ root, sourceControls, settingsControls,
  renderSettings, replaceProject, confirmReplace, urls = root.defaultView.URL }) {
  const find = id => root.querySelector("#" + id);
  const source = find("source"), filename = find("source-filename"),
    open = find("open-flow-project"), prepare = find("prepare-flow-project"),
    download = find("flow-project-download"), cancel = find("cancel-flow-project"),
    status = find("flow-project-status");
  if (![source, filename, open, prepare, download, cancel, status].every(Boolean)
      || typeof replaceProject !== "function" || typeof confirmReplace !== "function")
    throw new TypeError("Flow project controls and an explicit replacement handler are required.");
  let disposed = false, epoch = 0, reading = false, pending = null, prepared = null, url = null,
    finishing = null;
  const composing = new Set(), listeners = [];
  const fail = (code, message) => { throw new FlowError(code, message); };
  const alive = () => { if (disposed) fail("SESSION_DISPOSED", "Flow project controls are disposed."); };
  const compositionBusy = () => composing.size > 0 || finishing !== null;
  function refresh() {
    if (disposed) return;
    open.disabled = reading || compositionBusy() || access.blocked();
    prepare.disabled = compositionBusy();
    cancel.disabled = pending === null && prepared === null;
  }
  function revoke() {
    prepared = null; download.hidden = true;
    download.removeAttribute("href"); download.removeAttribute("download");
    if (url !== null) { const previous = url; url = null; urls.revokeObjectURL(previous); }
  }
  function invalidate(message = "Document or settings changed. Prepare the project again.") {
    epoch++; pending = null; revoke();
    if (!disposed && message) status.textContent = message;
    refresh();
  }
  const access = trackSourceAccess([source, filename], () => invalidate("Source access changed; reopen or prepare the project again."));
  function capture() {
    alive();
    if (compositionBusy()) fail("EDITOR_BUSY", "Finish text composition before saving or reopening a Flow project.");
    return captureFlowProject(sourceControls.snapshot(), settingsControls.capture());
  }
  function matches(value) {
    const current = capture();
    return current.document.source === value.document.source
      && current.document.filename === value.document.filename
      && JSON.stringify(current.settings) === JSON.stringify(value.settings);
  }
  function prepareDownload() {
    const value = capture();
    invalidate("");
    const ticket = epoch, output = flowProjectDownload(value.document, value.settings);
    const next = urls.createObjectURL(output.blob);
    try {
      if (disposed || ticket !== epoch || !matches(value))
        fail("STALE_PROJECT", "Document changed while preparing its Flow project.");
    } catch (error) { urls.revokeObjectURL(next); throw error; }
    url = next; prepared = value;
    download.href = url; download.download = output.filename;
    download.textContent = "Download " + output.filename; download.hidden = false;
    status.textContent = "Source and applied publishing settings prepared. Click Download to save the project; images and permissions are excluded.";
    refresh();
    return output;
  }
  async function openFile(file) {
    alive(); access.synchronize();
    if (reading) fail("DOCUMENT_BUSY", "The previous project file read has not settled.");
    if (access.blocked()) fail("EDITOR_BUSY", "Unlock the editor before reopening a Flow project.");
    const before = capture();
    invalidate("");
    const job = { epoch, before }; pending = job; reading = true;
    status.textContent = "Reading and validating the source project…"; refresh();
    function fence() {
      alive(); access.synchronize();
      if (pending !== job || job.epoch !== epoch || access.blocked() || !matches(before))
        fail("STALE_PROJECT", "The editor or settings changed; the project was not reopened.");
    }
    try {
      const project = await readFlowProjectFile(file);
      fence();
      const approved = await confirmReplace(project);
      fence();
      if (approved !== true) { status.textContent = "Project reopening declined; source and settings were kept."; return false; }
      // Host installation is synchronous: validate source and settings before
      // the existing source owner revokes images, history and old downloads.
      replaceProject(project);
      status.textContent = "Flow project reopened with its saved source and applied settings. Authorize its images again before publishing.";
      return true;
    } finally {
      if (pending === job) pending = null;
      reading = false; refresh();
    }
  }
  function report(error) {
    if (!disposed && error?.code !== "STALE_PROJECT")
      status.textContent = (error?.code ?? "PROJECT_ERROR") + ": " + (error?.message ?? "Project operation failed.") + " Current source and settings were retained.";
  }
  function listen(node, event, handler) {
    node.addEventListener(event, handler);
    listeners.push(() => node.removeEventListener(event, handler));
  }
  for (const node of [source, filename]) {
    listen(node, "input", () => invalidate());
    listen(node, "compositionstart", () => {
      if (finishing !== null) clearTimeout(finishing);
      finishing = null; composing.add(node); invalidate("Finish text composition before saving or reopening a project.");
    });
    listen(node, "compositionend", () => {
      composing.delete(node);
      if (finishing !== null) clearTimeout(finishing);
      finishing = setTimeout(() => { finishing = null; refresh(); }, 0);
      invalidate("Text composition completed. Prepare or reopen the project when ready.");
    });
  }
  listen(source, "fmd-document-replaced", () => invalidate("Document replaced; prepare a project for the new source."));
  listen(open, "change", () => {
    const file = open.files?.[0]; if (!file) return;
    void openFile(file).catch(report).finally(() => { if (!disposed) open.value = ""; });
  });
  listen(prepare, "click", () => { try { prepareDownload(); } catch (error) { revoke(); report(error); refresh(); } });
  listen(cancel, "click", () => invalidate("Project operation cancelled; source and settings were kept."));
  listen(download, "click", event => {
    try {
      if (!prepared || !matches(prepared)) fail("STALE_PROJECT", "The prepared Flow project is no longer current.");
    } catch {
      event.preventDefault(); invalidate("Source or settings changed; prepare the project again before downloading.");
    }
  });
  const unsubscribe = renderSettings.subscribe(() => invalidate());
  revoke(); refresh();
  return Object.freeze({
    prepareDownload, openFile,
    cancel() { invalidate("Project operation cancelled; source and settings were kept."); },
    dispose() {
      if (disposed) return;
      disposed = true; invalidate(""); access.dispose(); unsubscribe();
      if (finishing !== null) clearTimeout(finishing);
      finishing = null; composing.clear();
      for (const remove of listeners) remove();
      open.disabled = prepare.disabled = cancel.disabled = true;
    },
  });
}
