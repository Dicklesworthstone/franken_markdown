// File ownership only. Project validation and installation remain host-owned;
// this module never parses Markdown, renders, persists handles or grants assets.
export const BOOK_FILE_MAX_BYTES = 64 * 1024 * 1024;
export class BookFileError extends Error {
  constructor(code, message) { super(message); this.name = "BookFileError"; this.code = code; }
}
const fail = (code, message) => new BookFileError(code, message);
const equalBytes = (a, b) => a.length === b.length && a.every((byte, i) => byte === b[i]);
function filename(handle) {
  const name = handle?.name;
  if (handle?.kind !== "file" || typeof handle.getFile !== "function"
      || typeof name !== "string" || name.length > 255 || !/\.json$/i.test(name)
      || /[\x00-\x1f\x7f/\\]/.test(name))
    throw fail("INVALID_FILE_HANDLE", "Choose one local .json source-project file.");
  return name;
}
function publicError(cause) {
  if (cause instanceof BookFileError) return cause;
  const known = {
    AbortError: ["FILE_CANCELLED", "File access was cancelled; current book source is retained."],
    NotAllowedError: ["FILE_PERMISSION_DENIED", "File permission was denied. Use another file or prepare a download."],
    SecurityError: ["FILE_PERMISSION_DENIED", "File access requires a supported secure page and a user action."],
    NotFoundError: ["FILE_MISSING", "The connected file is unavailable. Keep editing or save a different file."],
    NoModificationAllowedError: ["FILE_LOCKED", "Another writer may own the file. Try again after it finishes."],
    QuotaExceededError: ["FILE_WRITE_FAILED", "Storage could not accept this save. Current book source is retained."],
  };
  if (Object.hasOwn(known, cause?.name)) return fail(...known[cause.name]);
  if (["INVALID_PROJECT", "INVALID_UNICODE", "INVALID_OPTIONS", "BOOK_LIMIT"].includes(cause?.code))
    return fail(cause.code, "The source project did not pass validation. The existing book and file connection are retained.");
  return fail("FILE_OPERATION_FAILED", "File access failed. Keep the book open and prepare a source download as a backup.");
}
function utf8(source, maximum) {
  if (typeof source !== "string" || source.length > maximum)
    throw fail("BOOK_LIMIT", "Source-project JSON exceeds the file byte limit.");
  // TextEncoder replaces isolated surrogates. Never silently save repaired text.
  for (let i = 0; i < source.length; i++) {
    const unit = source.charCodeAt(i);
    if (unit >= 0xd800 && unit <= 0xdbff) {
      const next = source.charCodeAt(++i);
      if (!(next >= 0xdc00 && next <= 0xdfff))
        throw fail("INVALID_UNICODE", "Source-project JSON must be well-formed Unicode.");
    } else if (unit >= 0xdc00 && unit <= 0xdfff)
      throw fail("INVALID_UNICODE", "Source-project JSON must be well-formed Unicode.");
  }
  const bytes = new TextEncoder().encode(source);
  if (bytes.length > maximum) throw fail("BOOK_LIMIT", "Source-project JSON exceeds the file byte limit.");
  return bytes;
}

/** read(): {source, key, stamp}; source is validated serialized project JSON,
 * key is its canonical semantic identity, stamp also fences uncaptured inputs.
 * decode(File): {project, key}; replace(project, stamp) is synchronous and must
 * install exactly that semantic identity. No callback receives a file handle.
 *
 * Exclusive browser streams plus byte comparisons are NOT an OS-wide CAS.
 * An external application can race the final comparison and close. Read-back
 * verifies the captured revision; an unverified close is never retried silently.
 */
export function createBookFileSession({ read, decode, replace, isBlocked = () => false,
  onState = () => {}, maxBytes = BOOK_FILE_MAX_BYTES }) {
  if (![read, decode, replace, isBlocked, onState].every(fn => typeof fn === "function")
      || !Number.isSafeInteger(maxBytes) || maxBytes < 1 || maxBytes > BOOK_FILE_MAX_BYTES)
    throw fail("INVALID_ARGUMENT", "File sessions require project callbacks and a bounded byte limit.");
  let connection = null, active = null, disposed = false, installing = false;
  let revision = 0, identity = 0, problem = null, lastError = null;
  let message = "No project file connected. Source download and portable backup remain available.";
  function snapshot() {
    const value = read();
    if (!value || typeof value.source !== "string" || typeof value.key !== "string"
        || typeof value.stamp !== "string" || value.source.length > maxBytes || value.key.length > maxBytes)
      throw fail("INVALID_PROJECT", "The host did not provide a bounded source-project snapshot.");
    return Object.freeze({ source: value.source, key: value.key, stamp: value.stamp });
  }
  const alive = () => { if (disposed) throw fail("SESSION_DISPOSED", "Project file session is disposed."); };
  function state() {
    let value = null;
    if (!disposed) { try { value = snapshot(); } catch { /* Invalid drafts stay editable. */ } }
    const dirty = !connection || !value || value.key !== connection.key || revision !== connection.savedRevision;
    return Object.freeze({ filename: connection?.name ?? null, busy: active?.kind ?? null,
      phase: disposed ? "disposed" : problem ?? (!value ? "invalid" : !connection ? "unlinked" : dirty ? "edited" : "saved"),
      dirty, valid: value !== null, message, error: lastError,
      canSave: !disposed && !active && !!connection && !!value && !problem && dirty && !isBlocked() });
  }
  function announce() { if (!disposed) { try { onState(state()); } catch { /* Observers cannot own I/O. */ } } }
  function abort(job) {
    if (job?.stream && !job.closing && !job.abortPromise) {
      try { job.abortPromise = Promise.resolve(job.stream.abort()).catch(() => {}); }
      catch { job.abortPromise = Promise.resolve(); }
    }
    return job?.abortPromise ?? Promise.resolve();
  }
  const owned = job => !disposed && job.identity === identity;
  function current(job) {
    if (!owned(job) || job.revision !== revision || isBlocked()) return false;
    try { const value = snapshot(); return value.key === job.snapshot.key && value.source === job.snapshot.source && value.stamp === job.snapshot.stamp; }
    catch { return false; }
  }
  function fence(job) {
    alive();
    if (!current(job)) throw fail("STALE_SOURCE", "The book changed during file access. No older source will be installed or committed.");
  }
  function changed() {
    if (disposed || installing) return;
    revision++; void abort(active); announce();
  }
  function detach() {
    if (disposed || installing) return;
    identity++; revision++; connection = null; problem = null; lastError = null;
    void abort(active);
    message = "Project file disconnected; current sources and resources were retained."; announce();
  }
  async function run(kind, work) {
    alive();
    if (active || isBlocked()) throw fail("FILE_BUSY", "Finish file access, importing or text composition first.");
    const job = { kind, snapshot: snapshot(), revision, identity, stream: null, closing: false,
      verified: false, associated: false, abortPromise: null };
    // Validate outgoing Unicode/size BEFORE offering a picker or touching disk.
    job.bytes = utf8(job.snapshot.source, maxBytes);
    active = job; lastError = null; message = `${kind} in progress. Editing remains available.`; announce();
    try { fence(job); return await work(job); }
    catch (cause) {
      let error = publicError(cause);
      if (!job.closing && !current(job)) error = fail(disposed ? "SESSION_DISPOSED" : "STALE_SOURCE",
        "The book or file connection changed; no older source was committed.");
      if (job.closing && !job.verified) error = fail("FILE_SAVE_UNCERTAIN",
        "The file may have changed, but this save could not be verified. Keep your edits; reload explicitly or save a different file.");
      if (owned(job)) {
        if (job.associated && error.code === "FILE_CHANGED") problem = "conflict";
        if (job.associated && error.code === "FILE_SAVE_UNCERTAIN") problem = "uncertain";
        lastError = error.code; message = `${error.code}: ${error.message}`;
      }
      throw error;
    } finally { await abort(job); if (active === job) active = null; announce(); }
  }
  async function disk(handle) {
    const name = filename(handle), file = await handle.getFile();
    if (!file || file.name !== name || filename(handle) !== name || typeof file.arrayBuffer !== "function"
        || !Number.isSafeInteger(file.size) || file.size < 0 || file.size > maxBytes)
      throw fail("FILE_CHANGED", "The selected file changed identity or exceeds the source-project byte limit.");
    const buffer = await file.arrayBuffer();
    if (!(buffer instanceof ArrayBuffer) || buffer.byteLength !== file.size)
      throw fail("FILE_CHANGED", "The file changed while its bytes were being read.");
    return { name, bytes: new Uint8Array(buffer), file };
  }
  async function compare(job, handle, expected) {
    const value = await disk(handle); fence(job);
    if (value.name !== expected.name || !equalBytes(value.bytes, expected.bytes))
      throw fail("FILE_CHANGED", "The project file changed on disk. Reload it explicitly or save a different file; nothing was overwritten.");
  }
  async function load(job, handle, confirm) {
    const value = await disk(handle); fence(job);
    // Decode the exact captured bytes, not a live file that may change again.
    const decoded = await decode(new File([value.bytes], value.name, { type: "application/json" })); fence(job);
    if (!decoded || typeof decoded.key !== "string" || decoded.key.length > maxBytes)
      throw fail("INVALID_PROJECT", "The selected file is not a valid source project.");
    if (typeof confirm !== "function" || await confirm(Object.freeze({ filename: value.name })) !== true) {
      fence(job); message = "Open declined. The current book and file connection are unchanged.";
      return Object.freeze({ opened: false });
    }
    fence(job); await compare(job, handle, value);
    installing = true;
    try {
      const result = replace(decoded.project, job.snapshot.stamp);
      if (result && typeof result.then === "function") throw fail("INVALID_ARGUMENT", "Project installation must be synchronous.");
      alive();
      if (snapshot().key !== decoded.key)
        throw fail("STALE_SOURCE", "The host did not install the selected project exactly; no file was connected.");
      identity++; revision++;
      connection = { handle, name: value.name, bytes: value.bytes, key: decoded.key, savedRevision: revision };
      problem = null; lastError = null;
    } catch (cause) { connection = null; problem = null; identity++; revision++; throw cause; }
    finally { installing = false; }
    message = `Opened ${value.name}. Only source/settings are connected; image and font access must be authorized separately.`;
    return Object.freeze({ opened: true, filename: value.name });
  }
  async function permission(handle) {
    if (typeof handle.requestPermission !== "function" || typeof handle.createWritable !== "function")
      throw fail("FILE_ACCESS_UNAVAILABLE", "This browser cannot save through this handle. Prepare a source download instead.");
    if (await handle.requestPermission({ mode: "readwrite" }) !== "granted")
      throw fail("FILE_PERMISSION_DENIED", "Write permission was not granted. Current book source is retained.");
  }
  async function write(job, handle, expected, adopt) {
    await compare(job, handle, expected);
    job.stream = await handle.createWritable({ keepExistingData: false, mode: "exclusive" }); fence(job);
    if (!job.stream || !["write", "close", "abort"].every(name => typeof job.stream[name] === "function"))
      throw fail("FILE_WRITE_FAILED", "The file handle did not provide a complete staged writer.");
    await compare(job, handle, expected);
    // Preserve a connected project's outer UTF-8 BOM; chapter text remains the
    // exact text admitted by the existing source-project serializer.
    const bom = job.associated && expected.bytes[0] === 239 && expected.bytes[1] === 187 && expected.bytes[2] === 191;
    if (job.bytes.length + (bom ? 3 : 0) > maxBytes) throw fail("BOOK_LIMIT", "Saved project plus BOM exceeds the byte limit.");
    const output = bom ? new Uint8Array(job.bytes.length + 3) : job.bytes;
    if (bom) { output.set([239, 187, 191]); output.set(job.bytes, 3); }
    await job.stream.write(new Blob([output], { type: "application/json", endings: "transparent" })); fence(job);
    await compare(job, handle, expected);
    job.closing = true; await job.stream.close();
    const saved = await disk(handle);
    if (saved.name !== expected.name || !equalBytes(saved.bytes, output))
      throw fail("FILE_SAVE_UNCERTAIN", "Read-back did not match the captured source project.");
    job.verified = true;
    const stillOwned = owned(job), stillCurrent = current(job);
    if (stillOwned && (job.associated || (adopt && stillCurrent))) {
      connection = { handle, name: saved.name, bytes: saved.bytes, key: job.snapshot.key, savedRevision: job.revision };
      problem = null; lastError = null;
    }
    if (stillOwned) message = stillCurrent
      ? `Saved and verified ${saved.name}. Images and fonts are not in this source-only file.`
      : `Saved the captured revision to ${saved.name}; newer edits were not saved or replaced.`;
    return Object.freeze({ saved: true, current: stillCurrent, connected: stillOwned && connection?.handle === handle, filename: saved.name });
  }
  return Object.freeze({
    get state() { return state(); },
    changed, detach,
    open(pick, confirm) {
      return run("Open project", async job => {
        if (typeof pick !== "function") throw fail("FILE_ACCESS_UNAVAILABLE", "Use the ordinary source-project file input in this browser.");
        const handles = await pick(); fence(job);
        if (!Array.isArray(handles) || handles.length !== 1) throw fail("INVALID_FILE_HANDLE", "Choose exactly one source-project file.");
        return load(job, handles[0], confirm);
      });
    },
    reload(confirm) {
      return run("Reload project", job => {
        if (!connection) throw fail("NO_CONNECTED_FILE", "Open or save a connected project file first.");
        job.associated = true; return load(job, connection.handle, confirm);
      });
    },
    save() {
      return run("Save project", async job => {
        if (!connection) throw fail("NO_CONNECTED_FILE", "Use Save source project as first.");
        if (problem) throw fail("FILE_CHANGED", "Resolve the disk conflict by reloading, or save to a different file.");
        job.associated = true;
        const target = connection; await permission(target.handle); fence(job);
        return write(job, target.handle, target, false);
      });
    },
    saveAs(pick, confirm) {
      return run("Save project as", async job => {
        if (typeof pick !== "function") throw fail("FILE_ACCESS_UNAVAILABLE", "Prepare a source-project download in this browser.");
        const handle = await pick(); fence(job); filename(handle);
        let associated = handle === connection?.handle;
        if (connection && !associated) {
          if (typeof handle.isSameEntry !== "function") throw fail("INVALID_FILE_HANDLE", "This handle cannot verify that the selected file is different.");
          associated = await handle.isSameEntry(connection.handle); fence(job);
          if (typeof associated !== "boolean") throw fail("INVALID_FILE_HANDLE", "The selected file returned an invalid identity response.");
        }
        job.associated = associated;
        if (associated && problem) throw fail("FILE_CHANGED", "Save as cannot bypass a conflict on the same file. Choose a different file.");
        await permission(handle); fence(job);
        const expected = associated ? connection : await disk(handle); fence(job);
        if (expected.bytes.length && (typeof confirm !== "function" || await confirm(Object.freeze({ filename: expected.name })) !== true)) {
          fence(job); message = "Overwrite declined. No source-project bytes were written.";
          return Object.freeze({ saved: false, current: false });
        }
        fence(job); return write(job, handle, expected, true);
      });
    },
    dispose() {
      if (disposed) return;
      disposed = true; identity++; revision++; connection = null; problem = null; message = ""; void abort(active);
    },
  });
}
