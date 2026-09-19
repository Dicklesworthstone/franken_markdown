// Report orchestration only: the existing Rust documentStats/accessibilityAudit
// APIs own Markdown interpretation. No JS parser, network or asset resolver.
export const BOOK_INSPECTION_LIMITS = Object.freeze({ chapters: 128, chapterBytes: 4 * 1024 * 1024,
  sourceBytes: 16 * 1024 * 1024, findings: 4096, headings: 4096, reportBytes: 2 * 1024 * 1024 });
const L = BOOK_INSPECTION_LIMITS;
const counters = ['bytes', 'lines', 'words', 'characters', 'sentences', 'syllables', 'reading_time_secs', 'speaking_time_secs'];
const structures = ['headings_total', 'paragraphs', 'code_blocks', 'tables', 'table_rows', 'table_cells', 'lists',
  'list_items', 'task_items_total', 'task_items_completed', 'blockquotes', 'math_blocks', 'math_inlines',
  'links_total', 'links_external', 'links_internal_anchors', 'images', 'footnote_definitions', 'footnote_references'];
const fail = (code, message) => Object.assign(new Error(message), { code });
const invalid = () => fail('INVALID_INSPECTION', 'The engine returned an invalid inspection report.');
function text(value, maximum) {
  if (typeof value !== 'string') throw invalid();
  let size = 0;
  if (value.length > maximum) throw fail('INSPECTION_LIMIT', 'Inspection text exceeds its byte budget.');
  for (const char of value) {
    const code = char.codePointAt(0);
    if (code >= 0xd800 && code <= 0xdfff) throw invalid();
    size += code < 128 ? 1 : code < 2048 ? 2 : code < 65536 ? 3 : 4;
    if (size > maximum) throw fail('INSPECTION_LIMIT', 'Inspection text exceeds its byte budget.');
  }
  return size;
}
function integer(value) { if (!Number.isSafeInteger(value) || value < 0) throw invalid(); return value; }
function array(value, maximum) {
  if (!Array.isArray(value)) throw invalid();
  if (value.length > maximum) throw fail('INSPECTION_LIMIT', 'Inspection contains too many findings or headings.');
  return value;
}
function sourceSnapshot(files) {
  array(files, L.chapters); if (!files.length) throw invalid();
  let size = 0; const seen = new Set();
  return Array.from(files, file => {
    const path = file?.path, source = file?.source;
    text(path, 1024); const sourceBytes = text(source, L.chapterBytes);
    if (!path || seen.has(path)) throw invalid(); seen.add(path);
    size += sourceBytes + text(path, 1024);
    if (size > L.sourceBytes) throw fail('INSPECTION_LIMIT', 'Inspection source exceeds 16 MiB.');
    return { path, source, sourceBytes };
  });
}
async function fingerprint(source) {
  if (!globalThis.crypto?.subtle) throw fail('INSPECTION_UNAVAILABLE', 'Source fingerprinting requires Web Crypto on a secure origin.');
  const bytes = await globalThis.crypto.subtle.digest('SHA-256', new TextEncoder().encode(source));
  return Array.from(new Uint8Array(bytes), byte => byte.toString(16).padStart(2, '0')).join('');
}
function finding(value, accessibility = false) {
  if (!value || typeof value !== 'object') throw invalid();
  text(value.code, 128); const message = accessibility ? value.detail : value.message;
  text(message, 4096); if (!value.code) throw invalid();
  // The accessibility ABI has no severity. Label it warning, not an invented
  // fatal PDF-conformance verdict. Preserve explicit source-lint severity.
  const severity = accessibility ? 'warning' : value.severity;
  if (!['info', 'warning', 'error'].includes(severity)) throw invalid();
  return { severity, code: value.code, message };
}
function stageBudget(check) {
  let bytes = 0;
  for (const item of [...check.findings, ...(check.outline ?? [])]) {
    for (const value of Object.values(item)) if (typeof value === 'string') {
      bytes += text(value, 128 * 1024 - bytes);
    }
  }
  return check;
}
function statistics(value, bytes) {
  if (!value || value.schema !== 'fmd-document-stats-v1' || !value.structure || typeof value.structure !== 'object') throw invalid();
  const metrics = Object.fromEntries(counters.map(key => [key, integer(value[key])]));
  if (metrics.bytes !== bytes) throw invalid();
  for (const key of ['flesch_reading_ease', 'flesch_kincaid_grade']) {
    if (!Number.isFinite(value[key])) throw invalid(); metrics[key] = value[key];
  }
  text(value.reading_ease_label, 256); metrics.reading_ease_label = value.reading_ease_label;
  const structure = Object.fromEntries(structures.map(key => [key, integer(value.structure[key])]));
  const outline = Array.from(array(value.outline, L.headings), heading => {
    if (!heading || !Number.isInteger(heading.level) || heading.level < 1 || heading.level > 6) throw invalid();
    text(heading.text, 4096); text(heading.slug, 4096);
    return { level: heading.level, text: heading.text, slug: heading.slug };
  });
  const findings = Array.from(array(value.findings, L.findings), value => finding(value));
  return stageBudget({ status: 'ok', metrics, structure, outline, findings });
}
function accessibility(value) {
  if (!value || value.schema_version !== '1' || value.target !== 'pdf') throw invalid();
  return stageBudget({ status: 'ok', findings: Array.from(array(value.findings, L.findings), value => finding(value, true)) });
}
function failure(error) {
  // Bound a plain Rust exception as well as Error/JSON exceptions; no stack or
  // arbitrary object serialization enters the report. A failed check is visible.
  let message = typeof error === 'string' ? error : error?.message;
  if (typeof message !== 'string' || !message) message = 'This check could not complete.';
  message = Array.from(message).slice(0, 512).join('').replace(/[\uD800-\uDFFF]/gu, '\ufffd');
  return { status: 'error', code: 'CHECK_FAILED', message };
}
function summary(chapters) {
  const total = Object.fromEntries(counters.map(key => [key, 0]));
  const findings = { info: 0, warning: 0, error: 0 }; let failedChecks = 0, analyzedChapters = 0;
  for (const chapter of chapters) {
    for (const check of [chapter.statistics, chapter.accessibility]) {
      if (check.status === 'error') { failedChecks++; continue; }
      for (const item of check.findings) findings[item.severity]++;
    }
    if (chapter.statistics.status === 'ok') {
      analyzedChapters++;
      for (const key of counters) total[key] = integer(total[key] + chapter.statistics.metrics[key]);
    }
  }
  return { total, findings, failedChecks, analyzedChapters, totalChapters: chapters.length,
    verdict: failedChecks ? 'incomplete' : Object.values(findings).some(Boolean) ? 'findings' : 'no-findings' };
}
function serialize(report) {
  // Admission before concatenating all escaped findings. Source bodies, assets,
  // font bytes and the PDF audit's text layer are deliberately not included.
  const parts = ['{"schema":"fmd-book-inspection-v1","scope":"unexpanded-source-chapters","chapters":['];
  let size = parts[0].length;
  for (const [index, chapter] of report.chapters.entries()) {
    const part = (index ? ',' : '') + JSON.stringify(chapter);
    size += text(part, L.reportBytes - size); parts.push(part);
  }
  const tail = '],"summary":' + JSON.stringify(report.summary) + '}\n';
  text(tail, L.reportBytes - size); parts.push(tail);
  return new TextEncoder().encode(parts.join(''));
}
/** Executed in a disposable worker. Stages run sequentially to bound concurrent
 * engine allocations; independent stage failures preserve useful other results.
 * This analyzes original source chapters, not transclusion-expanded publication.
 */
export async function inspectBook(engine, files) {
  const input = sourceSnapshot(files), chapters = []; let retainedBytes = 0;
  for (const file of input) {
    const chapter = { path: file.path, sourceBytes: file.sourceBytes, sourceSha256: await fingerprint(file.source) };
    try { chapter.statistics = statistics(await engine.documentStats(file.source), file.sourceBytes); }
    catch (error) { chapter.statistics = failure(error); }
    try { chapter.accessibility = accessibility(await engine.accessibilityAudit(file.source)); }
    catch (error) { chapter.accessibility = failure(error); }
    retainedBytes += text(JSON.stringify(chapter), L.reportBytes - retainedBytes);
    chapters.push(chapter);
  }
  const report = { chapters, summary: summary(chapters) };
  return { bytes: serialize(report), sourceLength: input.reduce((sum, file) => sum + file.sourceBytes, 0) };
}
function checkedStage(value, bytes, kind) {
  if (!value || typeof value !== 'object') throw invalid();
  if (value.status === 'error') {
    text(value.code, 128); text(value.message, 4096);
    return { status: 'error', code: value.code, message: value.message };
  }
  if (value.status !== 'ok') throw invalid();
  if (kind === 'statistics') return statistics({ ...value.metrics, schema: 'fmd-document-stats-v1', structure: value.structure,
    outline: value.outline, findings: value.findings }, bytes);
  return stageBudget({ status: 'ok', findings: Array.from(array(value.findings, L.findings), value => finding(value)) });
}
/** Validate the report and its source identities before exposing navigation or
 * downloads. Recompute aggregates instead of trusting an optimistic verdict.
 */
export async function readBookInspection(bytes, files) {
  const input = sourceSnapshot(files);
  if (!(bytes instanceof Uint8Array) || Object.prototype.toString.call(bytes.buffer) !== '[object ArrayBuffer]') throw invalid();
  if (!bytes.byteLength || bytes.byteLength > L.reportBytes) throw fail('INSPECTION_LIMIT', 'Inspection report exceeds its byte budget.');
  let value;
  try { value = JSON.parse(new TextDecoder('utf-8', { fatal: true }).decode(bytes)); } catch { throw invalid(); }
  if (!value || value.schema !== 'fmd-book-inspection-v1' || value.scope !== 'unexpanded-source-chapters'
      || !Array.isArray(value.chapters) || value.chapters.length !== input.length) throw invalid();
  const chapters = [];
  for (const [index, file] of input.entries()) {
    const chapter = value.chapters[index];
    if (!chapter || chapter.path !== file.path || chapter.sourceBytes !== file.sourceBytes
        || chapter.sourceSha256 !== await fingerprint(file.source)) throw fail('STALE_INSPECTION', 'The report belongs to different book source.');
    chapters.push({ path: file.path, sourceBytes: file.sourceBytes, sourceSha256: chapter.sourceSha256,
      statistics: checkedStage(chapter.statistics, file.sourceBytes, 'statistics'),
      accessibility: checkedStage(chapter.accessibility, file.sourceBytes, 'accessibility') });
  }
  return { schema: 'fmd-book-inspection-v1', scope: value.scope, chapters, summary: summary(chapters) };
}
export function inspectionDownload(report) {
  // Call with a report returned by readBookInspection; serialize the validated
  // copy, never the worker's unvalidated summary or unknown response fields.
  return new Blob([serialize(report)], { type: 'application/json' });
}
