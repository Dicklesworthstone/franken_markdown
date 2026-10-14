import { createBookSourceMerger } from "./book_source_merge.mjs";

// Explicit source-folder I/O. No Markdown parser, renderer, project installer,
// directory enumeration, asset grants, persisted handles, or file deletion.
// Optional refresh applies a reviewed source-only transaction through its host.
export const BOOK_DIRECTORY_LIMITS = Object.freeze({ files: 256, fileBytes: 4 * 1024 * 1024,
  sourceBytes: 16 * 1024 * 1024, pathBytes: 1024 });
export class BookDirectoryError extends Error {
  constructor(code, message) { super(message); this.name = "BookDirectoryError"; this.code = code; }
}
const fail = (code, message) => new BookDirectoryError(code, message);
const own = (value, key) => {
  const property = value && Object.getOwnPropertyDescriptor(value, key);
  if (!property || !Object.hasOwn(property, "value"))
    throw fail("INVALID_SOURCE", "Use owned source data, not accessors or sparse arrays.");
  return property.value;
};
const equal = (a, b) => a.length === b.length && a.every((item, i) => item === b[i]);
const same = (a, b) => a.stamp === b.stamp && a.files.length === b.files.length
  && a.files.every((file, i) => file.path === b.files[i].path && file.source === b.files[i].source);
const component = name => typeof name === "string" && name && name !== "." && name !== ".."
  && !/[\\/\x00-\x1f\x7f<>:"|?*#%]/.test(name) && !/[. ]$/.test(name)
  && !/^(?:con|prn|aux|nul|com[1-9]|lpt[1-9])(?:\.|$)/i.test(name);
function utf8(text, limit) {
  if (typeof text !== "string" || text.length > limit)
    throw fail("BOOK_LIMIT", "A source or path exceeds the source-folder byte limit.");
  for (let i = 0; i < text.length; i++) {
    const unit = text.charCodeAt(i);
    if (unit >= 0xd800 && unit <= 0xdbff) {
      const next = text.charCodeAt(++i);
      if (!(next >= 0xdc00 && next <= 0xdfff)) throw fail("INVALID_UNICODE", "Repair incomplete Unicode before saving source files.");
    } else if (unit >= 0xdc00 && unit <= 0xdfff)
      throw fail("INVALID_UNICODE", "Repair incomplete Unicode before saving source files.");
  }
  const bytes = new TextEncoder().encode(text);
  if (bytes.length > limit) throw fail("BOOK_LIMIT", "A source or path exceeds the source-folder byte limit.");
  return bytes;
}
function publicError(cause) {
  if (cause instanceof BookDirectoryError) return cause;
  const code = { NotAllowedError: "DIRECTORY_PERMISSION_DENIED", SecurityError: "DIRECTORY_PERMISSION_DENIED",
    AbortError: "DIRECTORY_CANCELLED", NotFoundError: "DIRECTORY_CHANGED", TypeMismatchError: "DIRECTORY_CHANGED",
    NoModificationAllowedError: "DIRECTORY_LOCKED", QuotaExceededError: "DIRECTORY_WRITE_FAILED" }[cause?.name];
  return fail(code ?? "DIRECTORY_OPERATION_FAILED", "Source-folder access failed. Keep the book open and retain a separate backup.");
}

/** read() supplies { files: [{path, source}], stamp }, including uncaptured
 * editor changes in stamp. bind(pick, confirm) reviews exactly these paths; it
 * neither imports nor writes. save(confirm) writes only the reviewed path set.
 * refresh(confirm, apply) merges whole-source disk changes after explicit review;
 * apply(files, expectedStamp) must synchronously publish one source transaction
 * and return the new read().stamp. It must not change source membership/identity.
 *
 * Each stream is staged, but closing several files is NOT an atomic transaction
 * or an OS-wide compare-and-swap. All files are checked before opening writers
 * and again before the first close. Every close is checked and read back. Failure
 * aborts unclosed streams and returns a receipt on the thrown error. New files
 * and parent directories may remain after cancellation; no deletion/rollback
 * can clobber an external editor's later work. Explicit review is then required.
 */
export function createBookDirectorySession({ read, isBlocked = () => false, onState = () => {},
  limits = BOOK_DIRECTORY_LIMITS }) {
  if (![read, isBlocked, onState].every(fn => typeof fn === "function"))
    throw fail("INVALID_ARGUMENT", "Source folders require a snapshot host and state callbacks.");
  const cap = { ...BOOK_DIRECTORY_LIMITS, ...limits };
  for (const key of Object.keys(BOOK_DIRECTORY_LIMITS))
    if (!Number.isSafeInteger(cap[key]) || cap[key] < 1 || cap[key] > BOOK_DIRECTORY_LIMITS[key])
      throw fail("INVALID_ARGUMENT", "Use positive source-folder limits no larger than the defaults.");
  let connection = null, active = null, serial = 0, disposed = false, problem = null, receipt = null;
  let message = "No source folder connected. Project downloads and portable backups remain separate.";
  function capture(encode = false) {
    const value = read(), raw = own(value, "files"), stamp = own(value, "stamp");
    if (!Array.isArray(raw) || own(raw, "length") > cap.files || typeof stamp !== "string")
      throw fail("INVALID_SOURCE", "Provide a bounded source list and an editor checkpoint.");
    const count = own(raw, "length"), paths = new Set(), files = [];
    let bytes = 0;
    for (let i = 0; i < count; i++) {
      const input = own(raw, String(i)), path = own(input, "path"), source = own(input, "source");
      if (typeof path !== "string" || path !== path.trim() || !path.split("/").every(component)
          || path.length > cap.pathBytes || typeof source !== "string" || source.length > cap.fileBytes)
        throw fail("INVALID_SOURCE", "Use bounded UTF-8 sources and portable relative file paths.");
      const key = path.normalize("NFC").toLowerCase();
      if (paths.has(key)) throw fail("PATH_COLLISION", "Case or Unicode-equivalent source paths cannot share a folder save.");
      paths.add(key);
      const output = encode ? utf8(source, cap.fileBytes) : null;
      bytes += encode ? output.length + utf8(path, cap.pathBytes).length : source.length + path.length;
      if (bytes > cap.sourceBytes) throw fail("BOOK_LIMIT", "The source-folder snapshot exceeds its aggregate byte limit.");
      files.push(Object.freeze({ path, source, ...(encode ? { bytes: output } : {}) }));
    }
    for (const key of paths) {
      let at = key.indexOf("/");
      while (at >= 0) {
        if (paths.has(key.slice(0, at))) throw fail("PATH_COLLISION", "A source file cannot also be another source's directory.");
        at = key.indexOf("/", at + 1);
      }
    }
    return Object.freeze({ files: Object.freeze(files), stamp });
  }
  const alive = () => { if (disposed) throw fail("SESSION_DISPOSED", "Source-folder access is disposed."); };
  const pathSetMatches = value => connection && value.files.length === connection.entries.size
    && value.files.every(file => connection.entries.has(file.path));
  function state() {
    let value = null;
    if (!disposed) { try { value = capture(); } catch { /* Preserve invalid editor drafts. */ } }
    const compatible = value !== null && pathSetMatches(value);
    const dirty = compatible ? value.files.filter(file => connection.entries.get(file.path).source !== file.source).length : null;
    return Object.freeze({ folder: connection?.name ?? null, busy: active?.kind ?? null,
      phase: disposed ? "disposed" : problem ?? (!value ? "invalid" : !connection ? "unlinked" : !compatible ? "needs-review" : dirty ? "edited" : "saved"),
      dirty, files: value?.files.length ?? 0, message, receipt,
      canSave: !disposed && !active && !problem && !!compatible && !!dirty && !isBlocked(),
      canRefresh: !disposed && !active && !!compatible && (!problem || problem === "conflict") && !isBlocked() });
  }
  const announce = () => { if (!disposed) { try { onState(state()); } catch { /* Observers cannot own disk I/O. */ } } };
  const current = job => {
    if (disposed || serial !== job.serial || isBlocked()) return false;
    try { return same(job.snapshot, capture()); } catch { return false; }
  };
  function fence(job) {
    alive();
    if (!current(job)) throw fail("STALE_SOURCE", "Source, editor state or folder ownership changed; remaining writes were stopped.");
  }
  function stop(job) {
    return Promise.all((job?.writes ?? []).map(item => {
      if (item.stream && !item.closing && !item.aborting) {
        try { item.aborting = Promise.resolve(item.stream.abort()).catch(() => { item.abortFailed = true; }); }
        catch { item.abortFailed = true; item.aborting = Promise.resolve(); }
      }
      return item.aborting;
    }));
  }
  function changed() { if (!disposed) { serial++; void stop(active); announce(); } }
  function detach() {
    if (disposed) return;
    connection = null; problem = null; serial++; void stop(active);
    message = "Source folder disconnected. No source, resource or disk file was removed."; announce();
  }
  function report(job, outcome) {
    return Object.freeze({ outcome, current: current(job), folder: job.target?.name ?? null,
      files: Object.freeze(job.writes.map(item => Object.freeze({ path: item.path,
        status: item.verified ? "verified" : item.closing ? "uncertain" : item.created ? "created-unverified" : item.abortFailed ? "abort-unverified" : "not-saved" }))),
      createdDirectories: Object.freeze([...job.directories]),
      verified: job.writes.filter(item => item.verified).length });
  }
  async function run(kind, work) {
    alive();
    if (active || isBlocked()) throw fail("DIRECTORY_BUSY", "Finish the current import, composition or folder operation first.");
    const job = { kind, snapshot: capture(true), serial, target: connection, writes: [], directories: [] };
    if (!job.snapshot.files.length) throw fail("NO_SOURCE_FILES", "Add source files before connecting a folder.");
    active = job; message = `${kind} in progress. ` + (kind === "Review external source changes"
      ? "Source changes require a completed review and explicit approval."
      : "No book source is replaced by folder access."); announce();
    try { fence(job); return await work(job); }
    catch (cause) {
      await stop(job);
      let error = publicError(cause);
      const uncertain = job.writes.some(item => item.closing && !item.verified);
      if (uncertain) error = fail("DIRECTORY_SAVE_UNCERTAIN", "A file close could not be verified. Review the receipt and folder before another save.");
      else if (error.code !== "DIRECTORY_REFRESH_UNCERTAIN" && !current(job)) error = fail(disposed ? "SESSION_DISPOSED" : "STALE_SOURCE", "The book or folder connection changed; remaining writes were stopped.");
      const result = report(job, uncertain ? "uncertain" : job.writes.some(item => item.verified) ? "partial" : "stopped");
      error.receipt = result;
      if (!disposed) {
        if (job.writes.length || !receipt) receipt = result;
        if (job.target && connection === job.target) {
          if (uncertain) problem = "uncertain";
          else if (error.code === "DIRECTORY_CHANGED" || error.code === "INVALID_UNICODE") problem = "conflict";
          else if (job.directories.length || job.writes.some(item => item.created || item.abortFailed)) problem = "needs-review";
        }
        message = `${error.code}: ${error.message} ${result.verified} file(s) verified; abort was requested for unclosed streams. No rollback or deletion was attempted.`;
      }
      throw error;
    } finally { await stop(job); if (active === job) active = null; announce(); }
  }
  function directory(handle, name) {
    if (handle?.kind !== "directory" || typeof handle.name !== "string" || handle.name.length > 255
        || (name !== undefined && handle.name !== name)
        || typeof handle.getDirectoryHandle !== "function" || typeof handle.getFileHandle !== "function")
      throw fail("INVALID_DIRECTORY_HANDLE", "Choose a local source directory with supported file access.");
    return handle;
  }
  async function locate(job, target, path, create = false, checkCurrent = true) {
    const parts = path.split("/");
    let parent = directory(target.root, target.name);
    for (let i = 0; i < parts.length - 1; i++) {
      try { parent = directory(await parent.getDirectoryHandle(parts[i]), parts[i]); }
      catch (error) {
        if (error?.name !== "NotFoundError") throw error;
        if (!create) return null;
        fence(job);
        job.directories.push(parts.slice(0, i + 1).join("/"));
        parent = directory(await parent.getDirectoryHandle(parts[i], { create: true }), parts[i]);
      }
      if (checkCurrent) fence(job);
    }
    let handle;
    try { handle = await parent.getFileHandle(parts.at(-1), { create }); }
    catch (error) { if (!create && error?.name === "NotFoundError") return null; throw error; }
    if (handle?.kind !== "file" || handle.name !== parts.at(-1) || typeof handle.getFile !== "function"
        || typeof handle.isSameEntry !== "function")
      throw fail("INVALID_FILE_HANDLE", "Source destinations must support file identity checks.");
    return handle;
  }
  async function disk(handle) {
    if (!handle) return { handle: null, bytes: null, source: null };
    const file = await handle.getFile();
    if (file?.name !== handle.name || !Number.isSafeInteger(file.size) || file.size < 0 || file.size > cap.fileBytes
        || typeof file.arrayBuffer !== "function") throw fail("DIRECTORY_CHANGED", "A source file changed identity or exceeds its byte limit.");
    const buffer = await file.arrayBuffer();
    if (!(buffer instanceof ArrayBuffer) || buffer.byteLength !== file.size)
      throw fail("DIRECTORY_CHANGED", "Source-file bytes changed while reading.");
    const bytes = new Uint8Array(buffer);
    let source;
    try { source = new TextDecoder("utf-8", { fatal: true, ignoreBOM: true }).decode(bytes); }
    catch { throw fail("INVALID_UNICODE", "A selected disk source is not UTF-8; no binary file will be overwritten."); }
    return { handle, bytes, source };
  }
  async function compare(job, target, path, expected) {
    const handle = await locate(job, target, path); fence(job);
    if (!!handle !== !!expected.handle || (handle && await handle.isSameEntry(expected.handle) !== true))
      throw fail("DIRECTORY_CHANGED", "A source path appeared, disappeared or changed file identity. Review the folder again.");
    fence(job);
    const value = await disk(handle); fence(job);
    if (handle && !equal(value.bytes, expected.bytes))
      throw fail("DIRECTORY_CHANGED", `Source file changed on disk: ${path}. It was not overwritten.`);
    return value;
  }
  async function aliases(job, entries) {
    const handles = [...entries.values()].filter(entry => entry.handle).map(entry => entry.handle);
    for (let i = 0; i < handles.length; i++) for (let j = 0; j < i; j++) {
      const identical = await handles[i].isSameEntry(handles[j]); fence(job);
      if (identical !== false) throw fail("PATH_COLLISION", "Two source paths resolve to the same or an unverifiable disk file.");
    }
  }
  const review = (job, target) => Object.freeze({ folder: target.name,
    files: Object.freeze(job.snapshot.files.map(file => Object.freeze({ path: file.path,
      action: target.entries.get(file.path).source === file.source ? "unchanged"
        : target.entries.get(file.path).handle ? "replace" : "create", bytes: file.bytes.length }))) });
  function refreshSources(confirm, apply, mergeText = false) {
    return run("Review external source changes", async job => {
      const target = job.target;
      if (!target) throw fail("NO_CONNECTED_DIRECTORY", "Review a source folder first.");
      if ((problem && problem !== "conflict") || !pathSetMatches(job.snapshot))
        throw fail("DIRECTORY_REVIEW_REQUIRED", "Review changed paths or uncertain disk writes before refreshing source.");
      if (typeof apply !== "function")
        throw fail("DIRECTORY_REFRESH_UNAVAILABLE", "The source host does not support atomic source-only refresh.");
      const entries = new Map(), next = [], changes = [];
      const merge = mergeText ? createBookSourceMerger() : null, decisions = new Map();
      let diskBytes = 0, mergedBytes = 0, conflicts = 0, changedSources = 0;
      for (const file of job.snapshot.files) {
        const value = await disk(await locate(job, target, file.path)); fence(job);
        const pathBytes = utf8(file.path, cap.pathBytes).length;
        diskBytes += (value.bytes?.length ?? 0) + pathBytes;
        if (diskBytes > cap.sourceBytes)
          throw fail("BOOK_LIMIT", "The disk source set exceeds the aggregate byte limit.");
        entries.set(file.path, value);
        const baseline = target.entries.get(file.path).source;
        let action, source = file.source, details = null;
        if (mergeText) {
          const plan = merge(baseline, file.source, value.source);
          action = { local: "keep-editor", disk: "reload" }[plan.action] ?? plan.action;
          if (action === "conflict") conflicts++;
          else { source = plan.source; if (source !== file.source) changedSources++; }
          const before = target.entries.get(file.path).handle;
          let identityChanged = !!before !== !!value.handle;
          if (before && value.handle) {
            const sameEntry = await value.handle.isSameEntry(before); fence(job);
            if (typeof sameEntry !== "boolean") throw fail("INVALID_FILE_HANDLE", "Disk identity could not be verified.");
            identityChanged = !sameEntry;
          }
          details = { base: baseline, local: file.source, disk: value.source,
            source: plan.source, reason: plan.reason, identityChanged };
        } else if (value.source === file.source) action = baseline === file.source ? "unchanged" : "converged";
        else if (value.source === baseline) action = "keep-editor";
        else if (value.source === null) { action = "missing"; conflicts++; }
        else if (file.source === baseline) { action = "reload"; source = value.source; changedSources++; }
        else { action = "conflict"; conflicts++; }
        mergedBytes += utf8(source, cap.fileBytes).length + pathBytes;
        if (!mergeText && mergedBytes > cap.sourceBytes)
          throw fail("BOOK_LIMIT", "Combining local and disk edits exceeds the aggregate source byte limit.");
        next.push(Object.freeze({ path: file.path, source }));
        changes.push(Object.freeze({ path: file.path, action, editorBytes: file.bytes.length,
          diskBytes: value.bytes?.length ?? null, ...(details ?? {}) }));
      }
      await aliases(job, entries);
      const review = Object.freeze({ folder: target.name, changedSources, conflicts,
        files: Object.freeze(changes) });
      if (conflicts && !mergeText) {
        problem = "conflict";
        const error = fail("DIRECTORY_SOURCE_CONFLICT", "Disk and editor changes cannot be combined without choosing a version. No source was replaced.");
        error.review = review; throw error;
      }
      const approval = typeof confirm === "function" ? await confirm(review) : false;
      fence(job);
      if (mergeText ? approval === false || approval === null || approval === undefined : approval !== true) {
        message = "Source refresh declined. Editor source, disk files and the old baseline are unchanged."; return null;
      }
      if (mergeText) {
        if (!Array.isArray(approval) || own(approval, "length") !== conflicts)
          throw fail("UNRESOLVED_SOURCE_CONFLICT", "Explicitly resolve every conflict before applying the reviewed source set.");
        const unresolved = new Map(changes.filter(file => file.action === "conflict").map(file => [file.path, file]));
        for (let i = 0; i < conflicts; i++) {
          const item = own(approval, String(i)), path = own(item, "path"), choice = own(item, "choice");
          const file = unresolved.get(path);
          if (!file || decisions.has(path) || (choice !== "local" && choice !== "disk")
              || (choice === "disk" && file.disk === null))
            throw fail("UNRESOLVED_SOURCE_CONFLICT", "Choose local or existing disk source exactly once for every conflict. No source is deleted.");
          decisions.set(path, choice);
        }
        mergedBytes = 0; changedSources = 0;
        for (let i = 0; i < next.length; i++) {
          const file = changes[i], choice = decisions.get(file.path);
          const source = choice ? file[choice] : next[i].source;
          mergedBytes += utf8(source, cap.fileBytes).length + utf8(file.path, cap.pathBytes).length;
          if (mergedBytes > cap.sourceBytes) throw fail("BOOK_LIMIT", "The resolved source set exceeds its aggregate byte limit.");
          next[i] = Object.freeze({ path: file.path, source });
          if (source !== job.snapshot.files[i].source) changedSources++;
        }
      }
      // Recheck the COMPLETE reviewed disk capture, not the old baseline.
      // A path replaced by an external atomic-save editor is allowed only if
      // that exact new identity/content was captured and remains unchanged.
      for (const file of job.snapshot.files) await compare(job, target, file.path, entries.get(file.path));
      fence(job);
      if (changedSources) {
        const original = job.snapshot;
        try {
          const stamp = apply(Object.freeze(next), original.stamp);
          if (stamp && typeof stamp.then === "function") {
            Promise.resolve(stamp).catch(() => {});
            throw fail("DIRECTORY_REFRESH_UNCERTAIN", "Source refresh requires a synchronous transaction; the folder was disconnected.");
          }
          const installed = capture();
          if (disposed || connection !== target || isBlocked() || typeof stamp !== "string"
              || stamp !== installed.stamp || installed.files.length !== next.length
              || installed.files.some((file, i) => file.path !== next[i].path || file.source !== next[i].source))
            throw fail("DIRECTORY_REFRESH_UNCERTAIN", "The source transaction could not be verified; inspect the current book before reconnecting.");
          // The host's ONE source transaction legitimately emits a collection
          // notification and advances its checkpoint. Adopt only the exact
          // verified result, never an arbitrary newer source capture.
          job.snapshot = installed; job.serial = serial;
        } catch (cause) {
          if (cause?.code !== "DIRECTORY_REFRESH_UNCERTAIN" && current(job)) throw cause;
          if (!disposed && connection === target) { connection = null; problem = null; serial++; }
          throw fail("DIRECTORY_REFRESH_UNCERTAIN", "Source may already have changed. The old folder authority was retired; inspect the book and reconnect explicitly.");
        }
      }
      target.entries = entries; problem = null;
      const result = Object.freeze({ outcome: changedSources ? "refreshed" : "unchanged",
        changedSources, current: current(job), folder: target.name,
        files: mergeText ? Object.freeze(changes.map(file => Object.freeze({ path: file.path, action: file.action,
          identityChanged: file.identityChanged, choice: decisions.get(file.path) ?? null, editorBytes: file.editorBytes, diskBytes: file.diskBytes }))) : review.files });
      message = mergeText
        ? `${changedSources} source(s) reconciled in one transaction. No disk files were written; local and merged differences still require explicit saving.`
        : `${changedSources} disk-edited source(s) applied as one source-only transaction. Local-only edits were retained. No disk file was written.`;
      return result;
    });
  }
  return Object.freeze({
    get state() { return state(); }, changed, detach,
    bind(pick, confirm) {
      return run("Review source folder", async job => {
        if (typeof pick !== "function") throw fail("DIRECTORY_ACCESS_UNAVAILABLE", "This browser cannot select writable source folders. Use project downloads instead.");
        const root = directory(await pick()); fence(job);
        const target = { root, name: root.name, entries: new Map() };
        job.target = target;
        let bytes = 0;
        for (const file of job.snapshot.files) {
          const value = await disk(await locate(job, target, file.path)); fence(job);
          bytes += (value.bytes?.length ?? 0) + utf8(file.path, cap.pathBytes).length;
          if (bytes > cap.sourceBytes) throw fail("BOOK_LIMIT", "The selected disk sources exceed the aggregate byte limit.");
          target.entries.set(file.path, value);
        }
        await aliases(job, target.entries);
        if (typeof confirm !== "function" || await confirm(review(job, target)) !== true) {
          fence(job); message = "Folder review declined. Existing connections and disk files are unchanged."; return false;
        }
        fence(job);
        for (const file of job.snapshot.files) await compare(job, target, file.path, target.entries.get(file.path));
        connection = target; problem = null; receipt = null;
        message = `Connected ${target.name} for ${target.entries.size} source paths. No disk file has been written. Save explicitly after reviewing changes.`;
        return true;
      });
    },
    /** Whole-source refresh preserves its original fail-on-conflict behavior. */
    refresh: (confirm, apply) => refreshSources(confirm, apply),
    /** Explicitly opt into bounded line merges and reviewed conflict choices.
     * Approval is an array of {path, choice: "local" | "disk"} covering every
     * conflict, or an empty array for a conflict-free review. false cancels.
     * Absent disk sources can only be kept locally; source deletion is separate.
     */
    reconcile: (confirm, apply) => refreshSources(confirm, apply, true),
    save(confirm) {
      return run("Save source files", async job => {
        const target = job.target;
        if (!target) throw fail("NO_CONNECTED_DIRECTORY", "Review a source folder first.");
        if (problem || !pathSetMatches(job.snapshot))
          throw fail("DIRECTORY_REVIEW_REQUIRED", "Source paths or disk state changed. Review and reconnect the folder before saving.");
        const changedFiles = job.snapshot.files.filter(file => file.source !== target.entries.get(file.path).source);
        job.writes = changedFiles.map(file => ({ path: file.path, file, expected: target.entries.get(file.path),
          created: false, stream: null, closing: false, verified: false }));
        if (changedFiles.length) {
          // Request permission while still in the initiating user action.
          if (typeof target.root.requestPermission !== "function"
              || await target.root.requestPermission({ mode: "readwrite" }) !== "granted")
            throw fail("DIRECTORY_PERMISSION_DENIED", "Write permission was not granted; source files were not saved.");
          fence(job);
          if (typeof confirm !== "function" || await confirm(review(job, target)) !== true) {
            fence(job); message = "Folder save declined; no disk files changed."; return null;
          }
          fence(job);
        }
        // Preflight the WHOLE reviewed source set, including unchanged files.
        for (const file of job.snapshot.files) await compare(job, target, file.path, target.entries.get(file.path));
        for (const item of job.writes) {
          fence(job);
          const file = item.file;
          if (!item.expected.handle) {
            item.created = true; // Even a rejected creation can have left an entry.
            const handle = await locate(job, target, file.path, true); fence(job);
            const empty = await disk(handle); fence(job);
            if (empty.bytes.length) throw fail("DIRECTORY_CHANGED", "A newly created source path acquired external content; it was not overwritten.");
            item.expected = empty;
          }
          const handle = item.expected.handle;
          if (typeof handle.createWritable !== "function") throw fail("DIRECTORY_ACCESS_UNAVAILABLE", "The browser cannot stage source-file writes.");
          item.stream = await handle.createWritable({ keepExistingData: false, mode: "exclusive" }); fence(job);
          if (!item.stream || !["write", "close", "abort"].every(key => typeof item.stream[key] === "function"))
            throw fail("DIRECTORY_WRITE_FAILED", "A file did not provide a complete staged writer.");
          await compare(job, target, file.path, item.expected);
          await item.stream.write(new Blob([file.bytes], { type: "text/plain;charset=utf-8", endings: "transparent" })); fence(job);
        }
        const expected = new Map(target.entries);
        for (const item of job.writes) expected.set(item.path, item.expected);
        await aliases(job, expected);
        for (const file of job.snapshot.files) await compare(job, target, file.path, expected.get(file.path));
        for (const item of job.writes) {
          fence(job); await compare(job, target, item.path, item.expected); fence(job);
          item.closing = true; await item.stream.close();
          // Do not fence away read-back of a close that has already started.
          const handle = await locate(job, target, item.path, false, false);
          if (!handle || await handle.isSameEntry(item.expected.handle) !== true)
            throw fail("DIRECTORY_SAVE_UNCERTAIN", "The saved path no longer identifies the staged source file.");
          const saved = await disk(handle);
          if (!equal(saved.bytes, item.file.bytes)) throw fail("DIRECTORY_SAVE_UNCERTAIN", "Read-back differs from the captured source bytes.");
          item.verified = true;
          if (!disposed && connection === target) target.entries.set(item.path, saved);
        }
        const result = report(job, job.writes.length ? "saved" : "unchanged");
        if (!disposed) { receipt = result; message = `${result.verified} source file(s) saved and verified.`
          + (result.current ? "" : " Newer editor changes were not saved.")
          + " Settings, images, fonts and unlisted disk files were not written or removed."; }
        return result;
      });
    },
    dispose() { if (!disposed) { detach(); disposed = true; } },
  });
}