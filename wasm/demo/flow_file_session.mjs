// Session-only local-file ownership. Handles never enter drafts, workers or
// source snapshots. The caller supplies pickers from a direct user gesture.
import { FlowError } from "../flow_session.mjs";
import { documentName, documentSnapshot, readMarkdownFile } from "./flow_document.mjs";

const error = (code, message) => new FlowError(code, message);
const same = (a, b) => a.filename === b.filename && a.source === b.source;
const sourceBlob = source => new Blob([source], { type: "text/markdown; charset=utf-8", endings: "transparent" });
function handleName(handle) {
  if (!handle || handle.kind !== "file" || typeof handle.getFile !== "function") {
    throw error("INVALID_FILE_HANDLE", "Choose a local Markdown file, not a directory.");
  }
  const name = documentName(handle.name);
  if (!/\.(md|markdown|txt)$/i.test(name)) {
    throw error("INVALID_FILENAME", "Direct file access accepts only .md, .markdown or .txt files.");
  }
  return name;
}
async function readHandle(handle) {
  const name = handleName(handle), file = await handle.getFile();
  const document = await readMarkdownFile(file);
  if (document.filename !== name || handle.name !== name) {
    throw error("FILE_CHANGED", "The selected file was renamed. Open it again before saving.");
  }
  return document;
}
function publicError(cause) {
  if (cause instanceof FlowError) return cause;
  const messages = {
    AbortError: ["FILE_CANCELLED", "The file operation was cancelled. Editor source is unchanged."],
    NotAllowedError: ["FILE_PERMISSION_DENIED", "File permission was denied. Use Save Markdown as or prepare a download."],
    SecurityError: ["FILE_PERMISSION_DENIED", "File access requires a secure page and an explicit user action."],
    NotFoundError: ["FILE_MISSING", "The selected file is unavailable. Keep editing or save to a different file."],
    NoModificationAllowedError: ["FILE_LOCKED", "Another writer may be using this file. Close it or save to a different file."],
    QuotaExceededError: ["FILE_WRITE_FAILED", "There is not enough storage to save the file. Your editor source is retained."]
  };
  const detail = Object.hasOwn(messages, cause?.name) ? messages[cause.name]
    : ["FILE_OPERATION_FAILED", "The file operation failed. Your editor source is retained; prepare a download as a backup."];
  return error(...detail);
}

/** Exact UTF-8 baselines, serialized writes and stale-source fences. This is not
 * an OS-wide compare-and-swap: another application can write between the final
 * comparison and close. Request exclusive browser writes, verify after close,
 * and never claim stronger guarantees or automatically retry an uncertain save.
 * replaceDocument/renameDocument are synchronous host transactions.
 */
export function createFileDocumentSession({ readDocument, replaceDocument, renameDocument,
  isBlocked = () => false, onState = () => {} }) {
  if (![readDocument, replaceDocument, renameDocument, isBlocked, onState].every(fn => typeof fn === "function")) {
    throw error("INVALID_ARGUMENT", "File sessions require document, rename, state and lifecycle callbacks.");
  }
  let connection = null, active = null, disposed = false, installing = false;
  let revision = 0, identity = 0, problem = null, message = "No file is connected. Import/download remains available.";
  const read = () => documentSnapshot(readDocument());
  const alive = () => { if (disposed) throw error("SESSION_DISPOSED", "File session is disposed."); };
  function state() {
    let current = null;
    if (!disposed) { try { current = read(); } catch { /* Invalid source stays editable. */ } }
    const renamed = !!connection && current?.filename !== connection.localName;
    const dirty = !connection || !current || current.source !== connection.source || renamed;
    return Object.freeze({ filename: connection?.diskName ?? null, busy: active?.kind ?? null,
      phase: disposed ? "disposed" : problem ?? (!current ? "invalid" : !connection ? "unlinked" : renamed ? "renamed" : dirty ? "edited" : "saved"),
      dirty, valid: current !== null, message,
      canSave: !disposed && !active && !!connection && !!current && !renamed && !problem && dirty });
  }
  function announce() {
    if (!disposed) { try { onState(state()); } catch { /* Observers cannot decide a disk transaction's outcome. */ } }
  }
  function abort(job) {
    if (job?.stream && !job.closing && !job.abortPromise) {
      try { job.abortPromise = Promise.resolve(job.stream.abort()).catch(() => {}); }
      catch { job.abortPromise = Promise.resolve(); }
    }
    return job?.abortPromise ?? Promise.resolve();
  }
  function changed() {
    if (disposed || installing) return;
    revision++; void abort(active);
    message = "Source changed. Only a completed, verified file save updates the disk baseline.";
    announce();
  }
  function detach() {
    if (disposed || installing) return;
    revision++; identity++; connection = null; problem = null; void abort(active);
    message = "File disconnected; source retained. Reopen explicitly to grant access again.";
    announce();
  }
  function currentJob(job) { return !disposed && job.identity === identity; }
  function unchanged(job) {
    if (!currentJob(job) || job.revision !== revision || isBlocked()) return false;
    try { return same(job.document, read()); } catch { return false; }
  }
  function fence(job) {
    alive();
    if (!unchanged(job)) throw error("STALE_SOURCE", "The editor or document changed during file access. No older source will be installed or committed.");
  }
  async function run(kind, work) {
    alive();
    if (active || isBlocked()) throw error("FILE_BUSY", "Finish the current file operation or text composition first.");
    const job = { kind, document: read(), revision, identity, stream: null, closing: false, verified: false, associated: false };
    active = job; message = `${kind} in progress. Source editing remains available.`; announce();
    try { return await work(job); }
    catch (cause) {
      let detail = publicError(cause);
      if (!job.closing && !unchanged(job)) {
        detail = disposed ? error("SESSION_DISPOSED", "File session is disposed.")
          : error("STALE_SOURCE", "The editor or document changed during file access. No older source was committed.");
      }
      if (job.closing && !job.verified) {
        detail = error("FILE_SAVE_UNCERTAIN", "The file may have changed, but this save could not be verified. Keep your edits; reload explicitly or save to a different file. Do not assume it was saved.");
      }
      if (currentJob(job)) {
        if (job.associated && detail.code === "FILE_CHANGED") problem = "conflict";
        if (job.associated && detail.code === "FILE_SAVE_UNCERTAIN") problem = "uncertain";
        message = `${detail.code}: ${detail.message}`;
      }
      throw detail;
    } finally {
      await abort(job);
      if (active === job) active = null;
      announce();
    }
  }
  async function compareDisk(job, handle, expected) {
    const disk = await readHandle(handle);
    fence(job);
    if (!same(disk, expected)) throw error("FILE_CHANGED", "The file changed on disk. Reload explicitly to use that version, or save your edits to a different file.");
  }
  function install(job, handle, document) {
    fence(job); installing = true;
    try {
      replaceDocument(document);
      if (!same(read(), document)) throw error("STALE_SOURCE", "The host did not install the selected source exactly; no file was connected.");
    } catch (cause) {
      connection = null; problem = null; identity++; revision++;
      throw cause;
    } finally { installing = false; }
    connection = { handle, source: document.source, localName: document.filename, diskName: document.filename };
    problem = null; identity++; revision++;
    message = `Opened ${document.filename}. File access is session-only; image access must be authorized separately.`;
    return Object.freeze({ opened: true, filename: document.filename });
  }
  async function load(job, handle, confirm) {
    handleName(handle);
    const document = await readHandle(handle); fence(job);
    if (typeof confirm !== "function" || await confirm(Object.freeze({ filename: document.filename })) !== true) {
      fence(job); message = "Open cancelled. The editor and current file connection were retained.";
      return Object.freeze({ opened: false });
    }
    fence(job);
    // Do not install a stale file when an external editor writes during confirmation.
    await compareDisk(job, handle, document);
    return install(job, handle, document);
  }
  async function writablePermission(handle) {
    if (typeof handle.requestPermission !== "function" || typeof handle.createWritable !== "function") {
      throw error("FILE_ACCESS_UNAVAILABLE", "This browser cannot save through this handle. Prepare a Markdown download instead.");
    }
    if (await handle.requestPermission({ mode: "readwrite" }) !== "granted") {
      throw error("FILE_PERMISSION_DENIED", "Write permission was not granted. Your source remains in the editor.");
    }
  }
  async function write(job, handle, expected, adopt) {
    await compareDisk(job, handle, expected);
    job.stream = await handle.createWritable({ keepExistingData: false, mode: "exclusive" });
    fence(job);
    if (!job.stream || !["write", "close", "abort"].every(name => typeof job.stream[name] === "function")) {
      throw error("FILE_WRITE_FAILED", "The browser did not provide a complete writable file stream.");
    }
    await compareDisk(job, handle, expected);
    await job.stream.write(sourceBlob(job.document.source)); fence(job);
    await compareDisk(job, handle, expected);
    job.closing = true;
    await job.stream.close();
    // Once close begins, newer edits cannot cancel it. Verify the captured source
    // and mark only that revision saved; never replace whatever is now in the editor.
    const saved = await readHandle(handle);
    if (saved.source !== job.document.source || saved.filename !== expected.filename) {
      throw error("FILE_SAVE_UNCERTAIN", "Read-back did not match the saved source.");
    }
    job.verified = true;
    const owned = currentJob(job);
    if (owned && job.associated) {
      connection = { ...connection, source: job.document.source }; problem = null;
    }
    let current = unchanged(job);
    if (adopt && current) {
      installing = true;
      try {
        renameDocument(saved.filename);
        const now = read();
        if (now.source !== job.document.source || now.filename !== saved.filename) {
          throw error("STALE_SOURCE", "The saved file was verified, but the editor changed; reopen it to connect.");
        }
        connection = { handle, source: saved.source, localName: saved.filename, diskName: saved.filename };
        problem = null; revision++; identity++;
      } catch {
        // The bytes were saved, but a failing host rename cannot authorize
        // subsequent overwrites for a possibly different editor document.
        connection = null; problem = null; revision++; identity++; current = false;
      } finally { installing = false; }
    }
    if (!disposed && owned) message = current ? `Saved and verified ${saved.filename}.`
      : `Saved the captured revision to ${saved.filename}; newer editor changes were not saved. No newer source was replaced.`;
    return Object.freeze({ saved: true, current, filename: saved.filename });
  }
  const api = {
    get state() { return state(); }, changed, detach,
    open(pick, confirm) {
      return run("Open", async job => {
        if (typeof pick !== "function") throw error("FILE_ACCESS_UNAVAILABLE", "Use the ordinary file input in this browser.");
        const handles = await pick(); fence(job);
        if (!Array.isArray(handles) || handles.length !== 1) throw error("INVALID_FILE_HANDLE", "Choose exactly one Markdown file.");
        return load(job, handles[0], confirm);
      });
    },
    reload(confirm) {
      return run("Reload", job => {
        if (!connection) throw error("NO_CONNECTED_FILE", "Open an editable file first.");
        job.associated = true;
        return load(job, connection.handle, confirm);
      });
    },
    save() {
      return run("Save", async job => {
        if (!connection) throw error("NO_CONNECTED_FILE", "Use Save Markdown as to choose a file first.");
        if (problem) throw error("FILE_CHANGED", "Resolve the disk conflict by reloading, or save to a different file.");
        if (job.document.filename !== connection.localName) throw error("FILENAME_CHANGED", "The source filename changed. Use Save Markdown as rather than overwrite the previously connected file.");
        job.associated = true;
        const handle = connection.handle, expected = { filename: connection.diskName, source: connection.source };
        await writablePermission(handle); fence(job);
        return write(job, handle, expected, false);
      });
    },
    saveAs(pick, confirmOverwrite) {
      return run("Save as", async job => {
        if (typeof pick !== "function") throw error("FILE_ACCESS_UNAVAILABLE", "Prepare a Markdown download in this browser.");
        const handle = await pick(job.document.filename); fence(job); handleName(handle);
        let associated = handle === connection?.handle;
        if (connection && !associated) {
          if (typeof handle.isSameEntry !== "function") throw error("INVALID_FILE_HANDLE", "The browser cannot verify that this is a different file.");
          associated = await handle.isSameEntry(connection.handle); fence(job);
          if (typeof associated !== "boolean") throw error("INVALID_FILE_HANDLE", "Invalid file identity response.");
        }
        job.associated = associated;
        if (associated && problem) throw error("FILE_CHANGED", "Save as cannot bypass a conflict on the same file. Choose a different file.");
        await writablePermission(handle); fence(job);
        const expected = associated ? { filename: connection.diskName, source: connection.source } : await readHandle(handle);
        fence(job);
        if (expected.source.length > 0) {
          if (typeof confirmOverwrite !== "function" || await confirmOverwrite(Object.freeze({ filename: expected.filename })) !== true) {
            fence(job); message = "Save as cancelled. No source bytes were written.";
            return Object.freeze({ saved: false, current: false });
          }
          fence(job);
        }
        return write(job, handle, expected, true);
      });
    },
    dispose() {
      if (disposed) return;
      disposed = true; revision++; identity++; connection = null; problem = null; message = "";
      void abort(active);
    }
  };
  return Object.freeze(api);
}
