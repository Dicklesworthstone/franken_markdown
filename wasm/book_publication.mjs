// Atomic multi-format publication over one private retained worker client.
// Capture and transport are injected by book-worker.js; this module neither
// parses Markdown nor serializes publication formats or creates object URLs.
const LIMIT = 128 * 1024 * 1024;
const TYPES = Object.freeze({
  pdf: ["application/pdf", "pdf"],
  epub: ["application/epub+zip", "epub"],
  site: ["application/zip", "zip"],
});
const DEFAULT_FORMATS = Object.freeze(["pdf", "epub", "site"]);
const error = (code, message) => Object.assign(new Error(message), { code });
const clock = () => typeof performance === "object" && typeof performance.now === "function"
  ? performance.now() : Date.now();

function publicationFormats(value) {
  const count = Array.isArray(value) ? Object.getOwnPropertyDescriptor(value, "length")?.value : -1;
  if (!Number.isInteger(count) || count < 1 || count > 3)
    throw error("INVALID_FORMAT", "Choose one through three unique publication formats: pdf, epub, site.");
  const formats = [];
  for (let i = 0; i < count; i++) {
    const field = Object.getOwnPropertyDescriptor(value, String(i));
    if (!field || !Object.hasOwn(field, "value") || typeof field.value !== "string"
        || !Object.hasOwn(TYPES, field.value) || formats.includes(field.value))
      throw error("INVALID_FORMAT", "Publication formats must be unique pdf, epub or site data entries.");
    formats.push(field.value);
  }
  return formats;
}

function publicationOutput(value, format, remaining, previousRevision, sourceLength) {
  const bytes = value?.bytes, revision = value?.retainedInputRevision;
  if (!Number.isSafeInteger(revision) || revision < 1)
    throw error("UNSUPPORTED_BOOK_PUBLICATION", "Rebuild the matching worker for captured multi-format publication.");
  if (revision <= previousRevision || value.format !== `book-${format}`
      || !Number.isSafeInteger(value.sourceLength) || value.sourceLength < 0
      || value.sourceLength > 64 * 1024 * 1024
      || (sourceLength !== null && value.sourceLength !== sourceLength)
      || !(bytes instanceof Uint8Array)
      || Object.prototype.toString.call(bytes.buffer) !== "[object ArrayBuffer]")
    throw error("WORKER_PROTOCOL_ERROR", "Publication output does not match the captured book and requested format.");
  if (bytes.byteLength === 0 || bytes.byteLength > remaining)
    throw error("OUTPUT_LIMIT", "Combined publication output is empty or exceeds its byte limit.");
  // Own exactly this view before another export or disposal can recycle its
  // backing storage. The aggregate limit is an output limit, not a heap bound.
  const owned = bytes.slice(), [mimeType, extension] = TYPES[format];
  return {
    revision,
    output: Object.freeze({
      format: `book-${format}`, bytes: owned, sourceLength: value.sourceLength,
      mimeType, extension, blob: () => new Blob([owned], { type: mimeType }),
    }),
  };
}

/** Internal, dependency-injected coordinator used by createBookPublicationWorker.
 * A publication owns a fresh retained client exclusively until all requested
 * outputs are admitted. Later formats use empty source deltas against its exact
 * acknowledged capture; images, fonts and unchanged sources are not resent.
 * request.strictLinks opts into the injected native navigation check before
 * any renderer is created. Verification shares the capture and deadline; there
 * is no fallback to unchecked output. The default remains non-strict.
 * When supplied, checkRetainedLinks must also validate the final native capture
 * before any result escapes. Public factories always install this second gate.
 * Worker loss may restart the WHOLE edition from this same owned input, within
 * the original deadline. No native mutation or partial output is replayed into
 * a surviving session. Invalid output/verification and cancellation never retry.
 * No partial output is published on failure, cancellation or timeout.
 */
export function createBookPublicationClient({
  createClient, captureInput, checkLinks, checkRetainedLinks, timeoutMs = 120000, maxOutputBytes = LIMIT,
  maxWorkerRetries = 1,
} = {}) {
  if (typeof createClient !== "function" || typeof captureInput !== "function"
      || (checkLinks !== undefined && typeof checkLinks !== "function")
      || (checkRetainedLinks !== undefined && typeof checkRetainedLinks !== "function")
      || !Number.isInteger(timeoutMs) || timeoutMs < 1 || timeoutMs > 600000
      || !Number.isInteger(maxOutputBytes) || maxOutputBytes < 1 || maxOutputBytes > LIMIT
      || !Number.isInteger(maxWorkerRetries) || maxWorkerRetries < 0 || maxWorkerRetries > 2)
    throw error("INVALID_OPTIONS", "Invalid publication factory, capture, deadline, output limit or worker retry limit.");
  let active = null, disposed = false;

  function render(files, formats = DEFAULT_FORMATS, options = {}, request = {}) {
    if (disposed) return Promise.reject(error("SESSION_DISPOSED", "Book publisher is disposed."));
    if (active) return Promise.reject(error("BOOK_BUSY", "A book publication is already running."));
    return new Promise((resolve, reject) => {
      let settled = false, client = null, timer = null, signal, onProgress, deadline = Infinity;
      let retryTimer = null, wakeRetry = null;
      let outputs = [], total = 0, sourceLength = null, revision = 0, linkReport;
      const abort = new AbortController();
      const retire = (owned) => {
        try { owned?.dispose(); } catch { /* Cleanup must not hide the primary result. */ }
      };
      const onAbort = () => finish(error("EXPORT_CANCELLED", "Book publication cancelled; no outputs were published."));
      const operation = { cancel: onAbort };
      const expired = () => finish(error("EXPORT_TIMEOUT", "Book publication exceeded its shared time budget."));
      const stopped = () => {
        // Timers cannot interrupt synchronous capture/copying or a custom
        // endpoint's microtask chain. Refuse late results at phase boundaries.
        if (!settled && clock() >= deadline) expired();
        return settled;
      };
      function progress(phase, attempt, completedFormats, format, retryCode) {
        if (stopped() || !onProgress) return;
        try {
          // No source, asset bytes, handles, partial output or verification
          // authority escapes through observers. Exceptions cannot fail a job.
          const pending = onProgress(Object.freeze({ phase, attempt,
            maxAttempts: maxWorkerRetries + 1, completedFormats, totalFormats: selected.length,
            ...(format === undefined ? {} : { format }),
            ...(retryCode === undefined ? {} : { retryCode }),
          }));
          pending?.catch?.(() => {});
        } catch { /* Observers may cancel/reenter, but cannot substitute results. */ }
      }
      function finish(cause, result, failed = true) {
        if (settled) return;
        settled = true;
        if (active === operation) active = null;
        clearTimeout(timer);
        clearTimeout(retryTimer);
        const wake = wakeRetry; wakeRetry = null; retryTimer = null; wake?.();
        try { signal?.removeEventListener("abort", onAbort); } catch { /* Still retire the worker. */ }
        const owned = client;
        client = null;
        outputs = [];
        input = null;
        // A publication owns the source-only checker as well as the renderer.
        // Cancel native verification even when no render client exists yet.
        if (failed) abort.abort(cause);
        retire(owned);
        if (failed) reject(cause);
        else resolve(result);
      }
      // Reserve before format proxies, option/source getters, signal adapters
      // or the worker factory can reenter this public operation.
      active = operation;
      let selected, input, strictLinks;
      try {
        selected = publicationFormats(formats);
        strictLinks = request?.strictLinks;
        if (strictLinks !== undefined && typeof strictLinks !== "boolean")
          throw error("INVALID_OPTIONS", "strictLinks must be a boolean.");
        if (strictLinks && typeof checkLinks !== "function")
          throw error("UNSUPPORTED_BOOK_VERIFICATION", "This publisher has no native navigation checker; rebuild the matching package.");
        onProgress = request?.onProgress;
        if (onProgress !== undefined && typeof onProgress !== "function")
          throw error("INVALID_OPTIONS", "onProgress must be a function.");
        signal = request?.signal;
        if (signal !== undefined && (signal === null || typeof signal.aborted !== "boolean"
            || typeof signal.addEventListener !== "function" || typeof signal.removeEventListener !== "function"))
          throw error("INVALID_OPTIONS", "signal must be an AbortSignal.");
        if (settled) return;
        if (signal?.aborted) { onAbort(); return; }
        signal?.addEventListener("abort", onAbort, { once: true });
        if (settled) return;
        if (signal?.aborted) { onAbort(); return; }
        // One deadline covers capture, initialization and every format, not a
        // fresh timeout per output. Disposing the client terminates Rust work.
        deadline = clock() + timeoutMs;
        timer = setTimeout(expired, timeoutMs);
        input = captureInput(files, options);
        if (stopped()) return;
        if (signal?.aborted) { onAbort(); return; }
      } catch (cause) {
        finish(cause);
        return;
      }
      void (async () => {
        for (let attempt = 1; !stopped(); attempt++) {
          try {
            if (strictLinks) {
              progress("checking-links", attempt, 0);
              if (stopped()) return;
              // Share immutable strings, never the mutable source containers or
              // asset/settings authority held for the subsequent publication.
              const sources = files => files.map(({ path, source }) => ({ path, source }));
              const checked = await checkLinks(sources(input.files), {
                includeSources: sources(input.options.includeSources ?? []),
                expandIncludes: input.options.expandIncludes,
              }, { signal: abort.signal });
              if (stopped()) return;
              const report = checked?.report;
              if (!Number.isSafeInteger(checked?.sourceLength) || checked.sourceLength < 0
                  || checked.sourceLength > 64 * 1024 * 1024
                  || report?.schema !== "fmd-book-link-report-v1" || report.scope !== "expanded-html-navigation"
                  || report.summary?.chapters !== input.files.length
                  || !Number.isSafeInteger(report.summary?.findings) || report.summary.findings < 0
                  || report.summary.findings > 4096)
                throw error("INVALID_LINK_REPORT", "Publication verification returned an invalid receipt.");
              linkReport = report;
              if (report.summary.findings > 0) {
                throw Object.assign(error("BOOK_LINKS_FAILED",
                  `Strict publication refused: ${report.summary.findings} internal navigation findings. Repair the book or explicitly disable strict checking.`), { linkReport });
              }
              // Every rendered format must attest the source byte metric that
              // the native checker verified, not merely agree with one another.
              sourceLength = checked.sourceLength;
            }
            const created = createClient({ strictLinks: strictLinks === true });
            if (stopped()) { retire(created); return; }
            client = created;
            if (!client || ["render", "renderSourceUpdate", "dispose"].some(key => typeof client[key] !== "function"))
              throw error("WORKER_FAILED", "Publication requires an owned retained source-update client.");
            for (let index = 0; index < selected.length; index++) {
              if (stopped()) return;
              if (total >= maxOutputBytes)
                throw error("OUTPUT_LIMIT", "No output budget remains for the next publication format.");
              const format = selected[index];
              progress("rendering", attempt, index, format);
              if (stopped()) return;
              const result = index === 0
                ? await client.render(input.files, format, input.options, { signal: abort.signal })
                : await client.renderSourceUpdate([], format, { expectedRevision: revision }, { signal: abort.signal });
              if (stopped()) return;
              const admitted = publicationOutput(result, format, maxOutputBytes - total, revision, sourceLength);
              if (stopped()) return;
              revision = admitted.revision;
              sourceLength = admitted.output.sourceLength;
              total += admitted.output.bytes.byteLength;
              outputs.push(admitted.output);
            }
            if (strictLinks && checkRetainedLinks) {
              progress("verifying-publication", attempt, selected.length);
              if (stopped()) return;
              // Verify the actual retained renderer, not just a second parse of
              // equally long source. All selected bytes remain private until this
              // revision-fenced native check agrees with the early preflight.
              const retained = await checkRetainedLinks(client, {
                paths: input.files.map(file => file.path), sourceLength,
                expectedRevision: revision,
              }, { signal: abort.signal });
              if (stopped()) return;
              if (retained?.summary?.findings > 0) {
                throw Object.assign(error("BOOK_LINKS_FAILED",
                  `Strict publication refused: ${retained.summary.findings} internal navigation findings in the retained publication.`),
                  { linkReport: retained });
              }
              if (!sameNavigationReport(linkReport, retained))
                throw error("BOOK_VERIFICATION_MISMATCH",
                  "The retained publication's navigation differs from its source preflight; no edition was published.");
              linkReport = retained;
            }
            const result = Object.freeze({
              format: "book-publication", sourceLength, byteLength: total,
              outputs: Object.freeze(outputs),
              ...(linkReport ? { linkReport } : {}),
            });
            finish(null, result, false);
            return;
          } catch (cause) {
            if (stopped()) return;
            let retryCode;
            try { retryCode = cause?.code; } catch { /* Keep the original failure. */ }
            // Only endpoint loss is recoverable. In particular, malformed
            // receipts, native transaction errors, stale revisions, findings,
            // unsupported binaries, output limits and timeouts stay failures.
            if (attempt > maxWorkerRetries
                || !["WORKER_FAILED", "BOOK_CAPTURE_EXPIRED"].includes(retryCode)) {
              finish(cause); return;
            }
            const owned = client; client = null;
            // Never combine files/reports from different native sessions. Drop
            // all private results, but keep the original normalized capture.
            outputs = []; total = 0; sourceLength = null; revision = 0; linkReport = undefined;
            retire(owned);
            progress("retrying", attempt + 1, 0, undefined, retryCode);
            if (stopped()) return;
            // Bounded backoff shares the same deadline and cancellation. Wake
            // the pending frame on finish so no abandoned retry keeps input alive.
            await new Promise(resolve => {
              wakeRetry = resolve;
              retryTimer = setTimeout(() => {
                retryTimer = null; wakeRetry = null; resolve();
              }, 100 * 2 ** (attempt - 1));
            });
          }
        }
      })();
    });
  }
  return Object.freeze({
    render,
    cancel() { active?.cancel(); },
    get busy() { return active !== null; },
    dispose() { disposed = true; active?.cancel(); },
  });
}

/** Source-only native-check adapter used by the public worker factory.
 * parseReport and textBytes are the EXISTING book-session decoder/UTF-8 policy,
 * not a second Markdown parser or optimistic worker-supplied summary. Separate
 * checker ownership avoids decoding/transferring publication image/font bytes.
 */
export function createBookPublicationLinkCheck({ createClient, parseReport, textBytes }) {
  if ([createClient, parseReport, textBytes].some(fn => typeof fn !== "function"))
    throw error("INVALID_OPTIONS", "Native publication checks require a worker and the book report decoder.");
  return async (files, options, { signal }) => {
    const paths = files.map(file => file.path);
    const sourceLength = [...files, ...options.includeSources]
      .reduce((sum, file) => sum + textBytes(file.source), 0);
    let client = null;
    const release = () => {
      const owned = client; client = null;
      try { owned?.dispose(); } catch { /* Preserve the check or cancellation result. */ }
    };
    const cancelled = () => {
      if (signal.aborted) throw signal.reason ?? error("EXPORT_CANCELLED", "Publication verification cancelled.");
    };
    try {
      cancelled();
      signal.addEventListener("abort", release, { once: true });
      client = createClient();
      cancelled();
      if (!client || ["render", "dispose"].some(key => typeof client[key] !== "function"))
        throw error("WORKER_FAILED", "Publication verification requires an owned checker client.");
      const output = await client.render(files, "links", options, { signal });
      cancelled();
      if (output?.format !== "book-links" || output.mimeType !== "application/json"
          || output.sourceLength !== sourceLength)
        throw error("INVALID_LINK_REPORT", "Navigation verification does not match the captured source.");
      const report = parseReport(output.bytes, paths);
      cancelled();
      return Object.freeze({ report, sourceLength });
    } finally {
      signal.removeEventListener("abort", release);
      release();
    }
  };
}


// Compare the admitted navigation contract, not raw JSON key order or a
// worker-supplied optimistic summary. Strict preflight has no findings. The
// decoder owns individual report validity and recomputes these counters.
function sameNavigationReport(before, after) {
  if (!before || !after || after.schema !== before.schema || after.scope !== before.scope
      || !before.summary || !after.summary
      || !Array.isArray(before.chapters) || !Array.isArray(after.chapters)
      || before.chapters.length !== after.chapters.length) return false;
  for (const key of ["chapters", "checked", "external", "unchecked", "findings"])
    if (after.summary[key] !== before.summary[key]) return false;
  return before.chapters.every((chapter, i) => {
    const other = after.chapters[i];
    return other && ["path", "checked", "external", "unchecked"].every(key => chapter[key] === other[key])
      && Array.isArray(chapter.findings) && chapter.findings.length === 0
      && Array.isArray(other.findings) && other.findings.length === 0;
  });
}

/** Final gate over the exact privately retained publication. The caller owns
 * the client: never dispose it here or create a replacement native book. Use
 * only an empty source delta and the last acknowledged capture revision. The
 * coordinator's deadline/abort disposes the endpoint on every failed outcome.
 */
export function createBookPublicationRetainedLinkCheck({ parseReport }) {
  if (typeof parseReport !== "function")
    throw error("INVALID_OPTIONS", "Retained publication verification requires the book report decoder.");
  return async (client, { paths, sourceLength, expectedRevision }, { signal }) => {
    const cancelled = () => {
      if (signal.aborted) throw signal.reason ?? error("EXPORT_CANCELLED", "Publication verification cancelled.");
    };
    cancelled();
    if (typeof client?.renderSourceUpdate !== "function")
      throw error("UNSUPPORTED_BOOK_VERIFICATION", "Rebuild the matching worker for retained publication verification.");
    const output = await client.renderSourceUpdate([], "links", { expectedRevision }, { signal });
    cancelled();
    const revision = output?.retainedInputRevision;
    if (!Number.isSafeInteger(revision) || revision <= expectedRevision)
      throw error("INVALID_LINK_REPORT", "Retained navigation verification lacks a fresh capture receipt.");
    if (output.format !== "book-links" || output.mimeType !== "application/json"
        || output.sourceLength !== sourceLength)
      throw error("INVALID_LINK_REPORT", "Retained navigation verification does not match the published capture.");
    const report = parseReport(output.bytes, paths);
    cancelled();
    return report;
  };
}
