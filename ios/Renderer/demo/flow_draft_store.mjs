// One opt-in source draft per origin. Compare-and-write happens entirely inside
// one IndexedDB readwrite transaction, never a localStorage read/modify/write.
import { FlowError } from "../flow_session.mjs";
import { documentSnapshot } from "./flow_document.mjs";

const STORE = "draft",
  KEY = "current";
const error = (code, message, cause) => new FlowError(code, message, { cause });
const storageError = (cause) =>
  error(
    cause?.name === "QuotaExceededError" ? "STORAGE_FULL" : "STORAGE_UNAVAILABLE",
    "Browser draft storage failed. Original Markdown remains in the editor; download it for safekeeping.",
    cause,
  );
function version(value) {
  if (!Number.isSafeInteger(value) || value < 0)
    throw error("INVALID_REVISION", "Invalid draft revision.");
  return value;
}
function decode(value) {
  if (value === undefined) return Object.freeze({ schemaVersion: 1, version: 0, document: null });
  try {
    if (
      !value ||
      value.schemaVersion !== 1 ||
      version(value.version) === 0 ||
      Object.keys(value).length !== 3 ||
      !Object.hasOwn(value, "document")
    )
      throw new Error("record schema");
    const document = value.document === null ? null : documentSnapshot(value.document);
    if (document && Object.keys(value.document).length !== 2) throw new Error("document schema");
    return Object.freeze({ schemaVersion: 1, version: value.version, document });
  } catch (cause) {
    throw error(
      "CORRUPT_DRAFT",
      "The stored draft is incompatible or corrupt. It has not been overwritten.",
      cause,
    );
  }
}
function connection(db, timeoutMs) {
  let closed = false;
  const pending = new Set();
  function dispose() {
    if (closed) return;
    closed = true;
    for (const abort of pending)
      abort(error("STORAGE_CLOSED", "The draft store was closed; reload storage before saving."));
    db.close();
  }
  db.onversionchange = dispose;
  db.onclose = dispose;
  function transaction(write, document, expected) {
    return new Promise((resolve, reject) => {
      if (closed) {
        reject(error("STORAGE_CLOSED", "The draft store is closed."));
        return;
      }
      let tx;
      try {
        tx = db.transaction(STORE, write ? "readwrite" : "readonly");
      } catch (cause) {
        reject(storageError(cause));
        return;
      }
      let failure = null,
        result;
      const abort = (reason) => {
        failure ??= reason;
        try {
          tx.abort();
        } catch {
          /* Commit/abort may already have started. */
        }
      };
      pending.add(abort);
      const timer = setTimeout(
        () => abort(error("STORAGE_TIMEOUT", "Draft storage did not acknowledge completion.")),
        timeoutMs,
      );
      const finish = () => {
        clearTimeout(timer);
        pending.delete(abort);
      };
      tx.onabort = () => {
        finish();
        reject(failure ?? storageError(tx.error));
      };
      // A put request succeeding is NOT a commit acknowledgment.
      tx.oncomplete = () => {
        finish();
        if (failure) reject(failure);
        else resolve(result);
      };
      const store = tx.objectStore(STORE),
        request = store.get(KEY);
      request.onsuccess = () => {
        try {
          const current = decode(request.result);
          if (!write) {
            result = current;
            return;
          }
          if (current.version !== expected)
            throw error(
              "DRAFT_CONFLICT",
              "Another tab changed this draft. Automatic saving is paused; neither version was overwritten.",
            );
          if (current.version === Number.MAX_SAFE_INTEGER)
            throw error("REVISION_EXHAUSTED", "Draft revision counter is exhausted.");
          result = Object.freeze({ schemaVersion: 1, version: current.version + 1, document });
          store.put(result, KEY);
        } catch (cause) {
          abort(cause instanceof FlowError ? cause : storageError(cause));
        }
      };
    });
  }
  return Object.freeze({
    read() {
      return transaction(false);
    },
    save(document, expectedVersion) {
      try {
        return transaction(true, documentSnapshot(document), version(expectedVersion));
      } catch (cause) {
        return Promise.reject(cause);
      }
    },
    clear(expectedVersion) {
      try {
        return transaction(true, null, version(expectedVersion));
      } catch (cause) {
        return Promise.reject(cause);
      }
    },
    dispose,
  });
}

export function openDraftStore({
  indexedDB = globalThis.indexedDB,
  name = "franken-markdown-source-draft-v1",
  timeoutMs = 5000,
} = {}) {
  return new Promise((resolve, reject) => {
    if (!indexedDB || typeof indexedDB.open !== "function") {
      reject(storageError());
      return;
    }
    if (
      typeof name !== "string" ||
      !name ||
      name.length > 128 ||
      !Number.isInteger(timeoutMs) ||
      timeoutMs < 1 ||
      timeoutMs > 60000
    ) {
      reject(error("INVALID_OPTIONS", "Invalid draft store name or timeout."));
      return;
    }
    let request,
      settled = false;
    const timer = setTimeout(
      () => fail(error("STORAGE_TIMEOUT", "Opening browser draft storage timed out.")),
      timeoutMs,
    );
    function fail(reason) {
      if (settled) return;
      settled = true;
      clearTimeout(timer);
      reject(reason);
    }
    try {
      request = indexedDB.open(name, 1);
    } catch (cause) {
      fail(storageError(cause));
      return;
    }
    request.onblocked = () =>
      fail(
        error(
          "STORAGE_BLOCKED",
          "Another tab is blocking draft storage. Close that tab and retry.",
        ),
      );
    request.onerror = () => fail(storageError(request.error));
    request.onupgradeneeded = (event) => {
      if (settled) {
        request.transaction.abort();
        return;
      }
      if (event.oldVersion !== 0) {
        request.transaction.abort();
        fail(error("CORRUPT_DRAFT", "Unsupported draft database version."));
        return;
      }
      request.result.createObjectStore(STORE);
    };
    request.onsuccess = () => {
      const db = request.result;
      // IDBOpenDBRequest cannot be cancelled. Always close a late success.
      if (settled) {
        db.close();
        return;
      }
      if (!db.objectStoreNames.contains(STORE)) {
        db.close();
        fail(error("CORRUPT_DRAFT", "Draft database is missing its record store."));
        return;
      }
      settled = true;
      clearTimeout(timer);
      resolve(connection(db, timeoutMs));
    };
  });
}
