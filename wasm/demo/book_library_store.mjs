// Persistent source projects only. The renderer, source editor and file downloads
// never depend on this store being available. No Web Storage or network fallback.

import { bookTextBytes } from "../book_session.mjs";
import { bookError } from "../book_worker.mjs";
import { normalizeBookProject, serializeBookProject } from "./book_collection.mjs";

export const BOOK_LIBRARY_LIMITS = Object.freeze({
  projects: 32,
  versions: 5,
  bytes: 128 * 1024 * 1024,
});
const STORES = ["entries", "snapshots", "meta"];
const conflict = () =>
  bookError(
    "LIBRARY_CONFLICT",
    "This saved book changed in another tab or was removed. Open its latest version or save a separate copy; no overwrite was accepted.",
  );
const unavailable = () =>
  bookError(
    "LIBRARY_UNAVAILABLE",
    "Local book storage is unavailable. Your editor is unchanged; download a source project instead.",
  );
const corrupt = () =>
  bookError(
    "LIBRARY_CORRUPT",
    "The saved book or library metadata is invalid. Nothing was replaced or deleted.",
  );
const validRevision = (value) => Number.isSafeInteger(value) && value > 0;
function checkedId(value) {
  if (typeof value !== "string" || !/^[a-zA-Z0-9_-]{1,80}$/.test(value))
    throw bookError("INVALID_PROJECT_ID", "Invalid local book identifier.");
  return value;
}
function checkedName(value) {
  if (typeof value !== "string" || !value.trim() || /[\x00-\x1f\x7f]/.test(value))
    throw bookError(
      "INVALID_NAME",
      "Give the saved book a nonempty name without control characters.",
    );
  const name = value.trim();
  bookTextBytes(name, 512);
  return name;
}
function storageError(error) {
  if (error?.code && typeof error.code === "string") return error;
  if (error?.name === "QuotaExceededError")
    return bookError(
      "LIBRARY_QUOTA",
      "The browser refused the storage quota. No save was acknowledged; download a source project and refresh recovery.",
    );
  return unavailable();
}
function checkedEntry(entry) {
  if (
    !entry ||
    !validRevision(entry.revision) ||
    !Number.isSafeInteger(entry.updatedAt) ||
    !Array.isArray(entry.versions) ||
    !entry.versions.length ||
    entry.versions.length > BOOK_LIBRARY_LIMITS.versions
  )
    throw corrupt();
  checkedId(entry.id);
  checkedName(entry.name);
  let previous = entry.revision + 1;
  for (const version of entry.versions) {
    if (
      !validRevision(version.revision) ||
      version.revision >= previous ||
      !Number.isSafeInteger(version.bytes) ||
      version.bytes <= 0 ||
      !Number.isSafeInteger(version.savedAt)
    )
      throw corrupt();
    checkedName(version.name);
    previous = version.revision;
  }
  if (entry.versions[0].revision !== entry.revision) throw corrupt();
  return entry;
}
function checkedUsage(value) {
  if (
    !value ||
    !Number.isSafeInteger(value.bytes) ||
    value.bytes < 0 ||
    !Number.isSafeInteger(value.count) ||
    value.count < 0
  )
    throw corrupt();
  return value;
}

/** Native IndexedDB transactions provide cross-tab compare-and-swap. Metadata
 * is separate from source payloads, so listing never reads every chapter body.
 * Limits may be lowered by hosts/tests, never raised past the public ceiling.
 */
export function createBookLibraryStore({
  indexedDB = globalThis.indexedDB,
  dbName = "franken-markdown-book-library-v1",
  makeId = () => globalThis.crypto.randomUUID(),
  now = () => Date.now(),
  timeoutMs = 15000,
  limits = BOOK_LIBRARY_LIMITS,
} = {}) {
  if (
    !Number.isInteger(timeoutMs) ||
    timeoutMs < 1 ||
    timeoutMs > 60000 ||
    typeof dbName !== "string" ||
    !dbName
  )
    throw unavailable();
  const cap = { ...BOOK_LIBRARY_LIMITS, ...limits };
  for (const key of Object.keys(BOOK_LIBRARY_LIMITS)) {
    if (!Number.isSafeInteger(cap[key]) || cap[key] < 1 || cap[key] > BOOK_LIBRARY_LIMITS[key])
      throw bookError("INVALID_OPTIONS", "Invalid book library limits.");
  }
  let closed = false,
    connection = null,
    opening = null,
    cancelOpen = null;
  const pending = new Set();
  const alive = () => {
    if (closed) throw bookError("LIBRARY_CLOSED", "The local book library is closed.");
  };
  function connect() {
    alive();
    if (connection) return Promise.resolve(connection);
    if (opening) return opening;
    if (!indexedDB || typeof indexedDB.open !== "function") return Promise.reject(unavailable());
    const promise = new Promise((resolve, reject) => {
      let request,
        settled = false;
      const finish = (error, db) => {
        if (settled) {
          db?.close();
          return;
        }
        settled = true;
        clearTimeout(timer);
        cancelOpen = null;
        if (error) reject(storageError(error));
        else resolve(db);
      };
      const timer = setTimeout(
        () =>
          finish(
            bookError(
              "LIBRARY_TIMEOUT",
              "Opening local storage timed out. Close other book-library tabs and retry.",
            ),
          ),
        timeoutMs,
      );
      cancelOpen = () => finish(bookError("LIBRARY_CLOSED", "The local book library is closed."));
      try {
        request = indexedDB.open(dbName, 1);
        request.onupgradeneeded = () => {
          if (settled || closed) {
            request.transaction.abort();
            return;
          }
          try {
            const db = request.result;
            db.createObjectStore("entries", { keyPath: "id" });
            db.createObjectStore("snapshots", { keyPath: "key" });
            db.createObjectStore("meta", { keyPath: "key" }).add({
              key: "usage",
              bytes: 0,
              count: 0,
            });
          } catch (error) {
            request.transaction.abort();
            finish(error);
          }
        };
        request.onerror = () => finish(request.error);
        request.onblocked = () =>
          finish(
            bookError(
              "LIBRARY_BLOCKED",
              "Another tab is blocking the book library. Close it and refresh recovery.",
            ),
          );
        request.onsuccess = () => {
          const db = request.result;
          if (settled || closed) {
            db.close();
            return;
          }
          db.onversionchange = () => {
            db.close();
            if (connection === db) connection = null;
          };
          db.onclose = () => {
            if (connection === db) connection = null;
          };
          connection = db;
          finish(null, db);
        };
      } catch (error) {
        finish(error);
      }
    });
    opening = promise;
    promise.then(
      () => {
        if (opening === promise) opening = null;
      },
      () => {
        if (opening === promise) opening = null;
      },
    );
    return promise;
  }
  async function transaction(mode, body) {
    const db = await connect();
    alive();
    return new Promise((resolve, reject) => {
      let tx,
        result,
        failure = null,
        settled = false,
        timer;
      const finish = (error) => {
        if (settled) return;
        settled = true;
        clearTimeout(timer);
        pending.delete(cancel);
        if (error) reject(storageError(error));
        else resolve(result);
      };
      const abort = (error) => {
        failure = error;
        try {
          tx.abort();
        } catch {
          finish(error);
        }
      };
      const cancel = () =>
        abort(
          bookError(
            "LIBRARY_CLOSED",
            "The local book library is closed. Refresh recovery before retrying an interrupted save.",
          ),
        );
      try {
        tx = db.transaction(STORES, mode);
        pending.add(cancel);
        // Request success is NOT a durable receipt. Only transaction completion
        // publishes a save/delete result; any request error aborts all stores.
        tx.oncomplete = () => finish(null);
        tx.onabort = () => finish(failure ?? tx.error ?? unavailable());
        tx.onerror = (event) => {
          failure ??= event.target.error ?? tx.error;
        };
        timer = setTimeout(
          () =>
            abort(
              bookError(
                "LIBRARY_TIMEOUT",
                "Storage did not acknowledge this operation in time. Refresh recovery before retrying.",
              ),
            ),
          timeoutMs,
        );
        const get = (store, key, next) => {
          const request = tx.objectStore(store).get(key);
          request.onsuccess = () => {
            try {
              next(request.result);
            } catch (error) {
              abort(error);
            }
          };
        };
        body({
          tx,
          get,
          done: (value) => {
            result = value;
          },
          abort,
        });
      } catch (error) {
        if (tx) abort(error);
        else finish(error);
      }
    });
  }
  return Object.freeze({
    async list() {
      return transaction("readonly", ({ tx, done, abort }) => {
        const request = tx.objectStore("entries").getAll();
        request.onsuccess = () => {
          try {
            done(
              request.result
                .map(checkedEntry)
                .sort((a, b) => b.updatedAt - a.updatedAt || a.id.localeCompare(b.id)),
            );
          } catch (error) {
            abort(error);
          }
        };
      });
    },
    async read(id, revision = null) {
      checkedId(id);
      if (revision !== null && !validRevision(revision))
        throw bookError("INVALID_REVISION", "Choose a saved revision.");
      return transaction("readonly", ({ get, done }) =>
        get("entries", id, (raw) => {
          if (!raw) throw bookError("LIBRARY_NOT_FOUND", "This saved book was removed.");
          const entry = checkedEntry(raw),
            selected = revision ?? entry.revision;
          const version = entry.versions.find((item) => item.revision === selected);
          if (!version)
            throw bookError(
              "LIBRARY_NOT_FOUND",
              "That revision is no longer retained. Refresh the library.",
            );
          get("snapshots", `${id}:${selected}`, (snapshot) => {
            if (
              !snapshot ||
              typeof snapshot.json !== "string" ||
              bookTextBytes(snapshot.json) !== version.bytes
            )
              throw corrupt();
            let project;
            try {
              project = normalizeBookProject(JSON.parse(snapshot.json));
            } catch {
              throw corrupt();
            }
            done({
              id,
              revision: entry.revision,
              snapshotRevision: selected,
              name: version.name,
              savedAt: version.savedAt,
              project,
            });
          });
        }),
      );
    },
    async save({ id = null, expectedRevision = null, name, project }) {
      // Snapshot and validate before even asynchronous database initialization.
      const json = serializeBookProject(project),
        bytes = bookTextBytes(json),
        label = checkedName(name);
      const creating = id === null;
      if (creating ? expectedRevision !== null : !validRevision(expectedRevision))
        throw bookError(
          "INVALID_REVISION",
          "Saving an existing book requires its observed revision.",
        );
      const key = checkedId(creating ? makeId() : id),
        savedAt = now();
      if (!Number.isSafeInteger(savedAt) || savedAt < 0 || savedAt > 8640000000000000)
        throw bookError("INVALID_OPTIONS", "Invalid save timestamp.");
      return transaction("readwrite", ({ tx, get, done }) =>
        get("entries", key, (raw) => {
          const old = raw && checkedEntry(raw);
          if (creating ? !!old : !old || old.revision !== expectedRevision) throw conflict();
          if (old?.revision === Number.MAX_SAFE_INTEGER)
            throw bookError("LIBRARY_LIMIT", "Revision limit reached. Save a separate copy.");
          get("meta", "usage", (rawUsage) => {
            const usage = checkedUsage(rawUsage),
              revision = (old?.revision ?? 0) + 1;
            const version = { revision, bytes, savedAt, name: label };
            const versions = [version, ...(old?.versions ?? [])],
              removed = [];
            let total = usage.bytes + bytes;
            // Prune only this book's old revisions, never another project's data.
            while (versions.length > 1 && (versions.length > cap.versions || total > cap.bytes)) {
              const victim = versions.pop();
              removed.push(victim);
              total -= victim.bytes;
            }
            const count = usage.count + (creating ? 1 : 0);
            if (total > cap.bytes || count > cap.projects)
              throw bookError(
                "LIBRARY_LIMIT",
                "Local library capacity reached. Download projects, then explicitly remove an unneeded saved book.",
              );
            const entry = { id: key, name: label, revision, updatedAt: savedAt, versions };
            tx.objectStore("snapshots").add({ key: `${key}:${revision}`, json });
            for (const victim of removed)
              tx.objectStore("snapshots").delete(`${key}:${victim.revision}`);
            tx.objectStore("entries").put(entry);
            tx.objectStore("meta").put({ key: "usage", bytes: total, count });
            done(entry);
          });
        }),
      );
    },
    async remove(id, expectedRevision) {
      checkedId(id);
      if (!validRevision(expectedRevision))
        throw bookError(
          "INVALID_REVISION",
          "Deleting a saved book requires its observed revision.",
        );
      return transaction("readwrite", ({ tx, get, done }) =>
        get("entries", id, (raw) => {
          const entry = raw && checkedEntry(raw);
          if (!entry || entry.revision !== expectedRevision) throw conflict();
          get("meta", "usage", (rawUsage) => {
            const usage = checkedUsage(rawUsage),
              bytes = usage.bytes - entry.versions.reduce((sum, version) => sum + version.bytes, 0);
            if (bytes < 0 || usage.count < 1) throw corrupt();
            for (const version of entry.versions)
              tx.objectStore("snapshots").delete(`${id}:${version.revision}`);
            tx.objectStore("entries").delete(id);
            tx.objectStore("meta").put({ key: "usage", bytes, count: usage.count - 1 });
            done({ id, deleted: true });
          });
        }),
      );
    },
    close() {
      if (closed) return;
      closed = true;
      cancelOpen?.();
      for (const cancel of [...pending]) cancel();
      connection?.close();
      connection = null;
    },
  });
}
