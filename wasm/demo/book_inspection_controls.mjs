import { readBookInspection, inspectionDownload } from '../book_inspection.mjs';
const fail = (code, message) => Object.assign(new Error(message), { code });
const PAGE_SIZE = 50;

/** Source inspection is explicit and owns its own cancellable worker. It never
 * controls publication, source saves, image grants or the editor's undo history.
 */
export function createBookInspectionControls({ root, controls, collection, worker, urls = URL }) {
  const ids = ['inspection-run', 'inspection-cancel', 'inspection-chapters', 'inspection-source', 'inspection-filter',
    'inspection-findings', 'inspection-previous', 'inspection-next', 'inspection-count', 'inspection-metrics',
    'inspection-outline', 'inspection-summary', 'inspection-status', 'inspection-save', 'inspection-download'];
  const el = Object.fromEntries(ids.map(id => [id, root.querySelector(`#${id}`)]));
  if (Object.values(el).some(value => !value)) throw new Error('Missing book inspection controls.');
  let disposed = false, suspended = false, busy = false, generation = 0, report = null, stamp = null;
  let selected = 0, page = 0, url = null;
  const listeners = [];
  function alive() { if (disposed || suspended) throw fail('INSPECTION_CLOSED', 'Inspection is not active.'); }
  function buttons() {
    const blocked = disposed || suspended || controls.sourceBusy;
    el['inspection-run'].disabled = blocked || busy || !collection.files.some(file => file.role !== "include");
    el['inspection-cancel'].disabled = disposed || suspended || (!busy && !report);
    for (const key of ['inspection-chapters', 'inspection-source', 'inspection-filter', 'inspection-save']) el[key].disabled = blocked || !report;
    el['inspection-previous'].disabled = blocked || !report || page === 0;
    el['inspection-next'].disabled = blocked || !report || (page + 1) * PAGE_SIZE >= rows().length;
  }
  function revoke() {
    el['inspection-download'].hidden = true; el['inspection-download'].removeAttribute('href');
    el['inspection-download'].removeAttribute('download');
    if (url !== null) { const old = url; url = null; urls.revokeObjectURL(old); }
  }
  function retire() {
    generation++; worker.cancel(); busy = false; report = null; stamp = null; page = 0; revoke();
    for (const key of ['inspection-chapters', 'inspection-findings', 'inspection-metrics', 'inspection-outline']) el[key].replaceChildren();
    el['inspection-summary'].textContent = ''; el['inspection-count'].textContent = '';
  }
  function error(error) {
    if (!disposed) el['inspection-status'].textContent = `${error.code ?? 'INSPECTION_ERROR'}: ${error.message ?? 'Inspection failed.'} Source, saves and publication exports are unchanged.`;
  }
  function current() {
    alive();
    if (!report || stamp !== controls.checkpoint()) {
      retire(); buttons(); throw fail('STALE_INSPECTION', 'The editor changed. Inspect the current book again.');
    }
    if (controls.sourceBusy) throw fail('BOOK_BUSY', 'Finish importing or composing text before navigating inspection results.');
  }
  function rows() {
    const chapter = report?.chapters[selected]; if (!chapter) return [];
    const result = [];
    for (const [key, label] of [['statistics', 'Structure'], ['accessibility', 'Accessibility']]) {
      const check = chapter[key];
      if (check.status === 'error') result.push({ severity: 'failed', label, code: check.code, message: check.message });
      else for (const finding of check.findings) result.push({ ...finding, label });
    }
    const filter = el['inspection-filter'].value;
    return !filter || filter === 'all' ? result : result.filter(item => item.severity === filter);
  }
  function paragraph(parent, text) { const p = root.createElement('p'); p.textContent = text; parent.append(p); }
  function openSource() {
    current();
    // Inspection indices count published chapters; editor indices also include
    // resources. Bind navigation by the validated chapter identity instead.
    const path = report.chapters[selected].path;
    const index = collection.files.findIndex(file => file.role !== "include" && file.path === path);
    if (index < 0) throw fail('STALE_INSPECTION', 'The inspected chapter is no longer in this book.');
    controls.selectSourceRange(index, 0, 0, stamp);
    // Navigating the active chapter changes the host checkpoint, not source.
    stamp = controls.checkpoint();
    el['inspection-status'].textContent = `Opened ${report.chapters[selected].path}. Findings have chapter scope; the engine does not provide exact source spans for these checks.`;
  }
  function render() {
    if (!report) { buttons(); return; }
    const chapter = report.chapters[selected], stats = chapter.statistics;
    el['inspection-metrics'].replaceChildren(); el['inspection-outline'].replaceChildren();
    if (stats.status === 'ok') {
      const m = stats.metrics, s = stats.structure;
      paragraph(el['inspection-metrics'], `${m.words} words; ${m.lines} source lines; ${m.bytes} UTF-8 bytes. Estimated reading ${m.reading_time_secs} seconds; speaking ${m.speaking_time_secs} seconds.`);
      paragraph(el['inspection-metrics'], `Readability estimate: ${m.reading_ease_label}; Flesch ${m.flesch_reading_ease}; grade ${m.flesch_kincaid_grade}. Heuristics, not language-independent quality scores.`);
      paragraph(el['inspection-metrics'], `${s.headings_total} headings; ${s.paragraphs} paragraphs; ${s.tables} tables; ${s.code_blocks} code blocks; ${s.images} images; ${s.links_total} links; ${s.task_items_completed}/${s.task_items_total} completed tasks.`);
      // Never invent a source span by searching for a heading's displayed text.
      const outline = root.createElement('pre'); outline.textContent = stats.outline.slice(0, 200).map(h => `${'  '.repeat(h.level - 1)}${h.text}`).join('\n') || 'No headings reported.';
      el['inspection-outline'].append(outline);
      if (stats.outline.length > 200) paragraph(el['inspection-outline'], `Showing 200 of ${stats.outline.length} headings. The downloaded report includes the full admitted outline.`);
    } else paragraph(el['inspection-metrics'], 'Statistics did not complete for this chapter. No zero-valued substitute is shown.');
    const findings = rows(); page = Math.min(page, Math.max(0, Math.ceil(findings.length / PAGE_SIZE) - 1));
    el['inspection-findings'].replaceChildren();
    for (const item of findings.slice(page * PAGE_SIZE, (page + 1) * PAGE_SIZE)) {
      const row = root.createElement('li');
      const label = root.createElement('p'); label.textContent = `${item.label} / ${item.severity} / ${item.code}: ${item.message}`;
      const button = root.createElement('button'); button.type = 'button'; button.textContent = 'Open chapter source';
      button.addEventListener('click', () => invoke(openSource)); row.append(label, button); el['inspection-findings'].append(row);
    }
    el['inspection-count'].textContent = findings.length ? `${page * PAGE_SIZE + 1}–${Math.min((page + 1) * PAGE_SIZE, findings.length)} of ${findings.length} matching items in ${chapter.path}.`
      : 'No items match this filter. Check the whole-book summary for failed checks and other chapters.';
    buttons();
  }
  function overview() {
    const s = report.summary;
    const verdict = s.verdict === 'incomplete' ? 'INCOMPLETE' : s.verdict === 'findings' ? 'FINDINGS TO REVIEW' : 'NO FINDINGS FROM THESE CHECKS';
    el['inspection-summary'].textContent = `${verdict}. ${s.failedChecks} failed checks; ${s.findings.error} errors, ${s.findings.warning} warnings, ${s.findings.info} informational findings. Statistics cover ${s.analyzedChapters}/${s.totalChapters} source chapters: ${s.total.words} words; estimated reading ${s.total.reading_time_secs} seconds. This is not a publication-conformance verdict.`;
    el['inspection-chapters'].replaceChildren(...report.chapters.map((chapter, i) => {
      const option = root.createElement('option'); option.value = String(i);
      const count = [chapter.statistics, chapter.accessibility].reduce((n, c) => n + (c.status === 'ok' ? c.findings.length : 1), 0);
      option.textContent = `${i + 1}. ${chapter.path} (${count} findings/failed checks)`; return option;
    }));
    selected = Math.min(selected, report.chapters.length - 1); el['inspection-chapters'].value = String(selected); render();
  }
  async function run() {
    let ticket = null;
    try {
      alive(); if (controls.sourceBusy) throw fail('BOOK_BUSY', 'Finish importing or composing text before inspection.');
      retire(); const project = controls.captureProject(), captured = controls.checkpoint();
      const files = project.files.filter(file => file.role !== "include");
      if (!files.length) throw fail('EMPTY_BOOK', 'Add a published chapter before running chapter inspection.');
      ticket = ++generation; busy = true; buttons();
      el['inspection-status'].textContent = 'Running Rust source-structure and accessibility checks in a separate worker. Cancel remains available; no export is being created.';
      // No image bytes, font grants, or publication settings cross this route:
      // the current engine audit is source-only with default PDF options.
      const result = await worker.render(files, 'inspection');
      const fence = () => {
        if (disposed || suspended || ticket !== generation || captured !== controls.checkpoint()) throw fail('STALE_INSPECTION', 'The book changed while inspection was running.');
      };
      fence(); if (result.format !== 'book-inspection') throw fail('INVALID_INSPECTION', 'The worker returned a different result type.');
      const checked = await readBookInspection(result.bytes, files); fence();
      report = checked; stamp = captured; busy = false; page = 0; overview();
      el['inspection-status'].textContent = 'Inspection finished. Review each chapter and any failed checks. Source findings do not validate transclusion-expanded book links, authorized images, or final PDF/EPUB conformance.';
      return structuredClone(report);
    } catch (e) {
      if (!disposed && (ticket === null || ticket === generation)) { retire(); error(e); buttons(); }
      throw e;
    } finally { if (!disposed && ticket === generation) { busy = false; buttons(); } }
  }
  function changed() {
    if (disposed) return; retire(); buttons();
    el['inspection-status'].textContent = 'Book or editor state changed. The previous report and its download were retired; run inspection again.';
  }
  function invoke(fn) { try { fn(); } catch (e) { error(e); } }
  function on(element, type, listener) { element.addEventListener(type, listener); listeners.push(() => element.removeEventListener(type, listener)); }
  const unsubscribe = collection.subscribe(changed);
  const unsubscribeState = controls.subscribeSourceState(() => { if (controls.sourceBusy) changed(); else buttons(); });
  for (const id of ['source-role', 'chapter-source', 'chapter-path', 'chapters', 'title', 'author', 'lang', 'font', 'dark-mode', 'font-scale', 'toc', 'page-numbers']) {
    const input = root.querySelector(`#${id}`); on(input, 'input', changed); on(input, 'change', changed);
  }
  on(el['inspection-run'], 'click', () => { void run().catch(() => {}); });
  on(el['inspection-cancel'], 'click', () => { retire(); buttons(); el['inspection-status'].textContent = 'Inspection cancelled and cleared. Source and publication exports are unchanged.'; });
  on(el['inspection-source'], 'click', () => invoke(openSource));
  on(el['inspection-chapters'], 'change', () => invoke(() => {
    current(); const next = Number(el['inspection-chapters'].value);
    if (!Number.isInteger(next) || next < 0 || next >= report.chapters.length) throw fail('INVALID_SELECTION', 'Choose an inspected chapter.');
    selected = next; page = 0; render();
  }));
  on(el['inspection-filter'], 'change', () => invoke(() => { current(); page = 0; render(); }));
  on(el['inspection-previous'], 'click', () => invoke(() => { current(); page = Math.max(0, page - 1); render(); }));
  on(el['inspection-next'], 'click', () => invoke(() => { current(); page++; render(); }));
  on(el['inspection-save'], 'click', () => invoke(() => {
    current(); revoke(); url = urls.createObjectURL(inspectionDownload(report));
    el['inspection-download'].href = url; el['inspection-download'].download = 'book-inspection.json'; el['inspection-download'].hidden = false;
    el['inspection-status'].textContent = 'Report prepared. Click Download inspection report to save it. It contains filenames, source fingerprints, headings and findings; review it before sharing.';
  }));
  on(el['inspection-download'], 'click', event => { try { current(); if (!url) throw fail('STALE_INSPECTION', 'Prepare the report again.'); } catch (e) { event.preventDefault(); error(e); } });
  retire(); buttons();
  return Object.freeze({ run,
    suspend() { if (disposed) return; suspended = true; retire(); buttons(); },
    resume() { if (disposed) return; suspended = false; buttons(); },
    dispose() { if (disposed) return; disposed = true; retire(); unsubscribe(); unsubscribeState(); for (const remove of listeners) remove(); worker.dispose(); }
  });
}

/** Add the optional inspection surface without replacing the publisher's
 * existing document, import, search, preview or export controls. All UI strings
 * are static; source-derived values are subsequently assigned as text only. */
export function createBookInspectionPanel(root) {
  if (root.querySelector('#inspection-panel')) throw new Error('Inspection panel already exists.');
  function node(tag, attributes, ...children) {
    const element = root.createElement(tag);
    for (const [key, value] of Object.entries(attributes ?? {})) element.setAttribute(key, value);
    for (const child of children) element.append(typeof child === 'string' ? root.createTextNode(child) : child);
    return element;
  }
  const button = (id, label, disabled = true) => node('button', { id, type: 'button', ...(disabled ? { disabled: '' } : {}) }, label);
  const select = node('select', { id: 'inspection-filter', disabled: '' }, ...[
    ['all', 'All findings and failed checks'], ['error', 'Source errors'], ['warning', 'Warnings'], ['info', 'Information'], ['failed', 'Failed checks']
  ].map(([value, label]) => node('option', { value }, label)));
  const panel = node('section', { id: 'inspection-panel', 'aria-labelledby': 'inspection-title' },
    node('h2', { id: 'inspection-title' }, 'Inspect source quality and accessibility'),
    node('p', { id: 'inspection-help' }, 'Run the Rust engine’s structural analysis and authoring-time accessibility checks on every original published chapter. Include-only sources are excluded from these chapter-scoped checks. This does not validate transclusion-expanded book links, image grants, or final publication conformance. Checks are manual and do not block exporting.'),
    node('p', {}, button('inspection-run', 'Inspect all source chapters', false), button('inspection-cancel', 'Cancel and clear inspection')),
    node('p', { id: 'inspection-status', role: 'status' }, 'Add chapters, then run inspection before publishing. Source is never uploaded.'),
    node('p', { id: 'inspection-summary', 'aria-live': 'polite' }),
    node('label', { for: 'inspection-chapters' }, 'Inspected chapter'),
    node('select', { id: 'inspection-chapters', disabled: '', 'aria-describedby': 'inspection-help' }),
    node('p', {}, button('inspection-source', 'Open this chapter’s source')),
    node('div', { id: 'inspection-metrics' }),
    node('details', {}, node('summary', {}, 'Source chapter outline'), node('div', { id: 'inspection-outline' })),
    node('label', { for: 'inspection-filter' }, 'Finding filter'), select,
    node('p', { id: 'inspection-count' }), node('ul', { id: 'inspection-findings' }),
    node('p', {}, button('inspection-previous', 'Previous 50 items'), button('inspection-next', 'Next 50 items')),
    node('p', {}, button('inspection-save', 'Prepare inspection report')),
    node('p', {}, node('a', { id: 'inspection-download', hidden: '' }, 'Download inspection report')),
    node('p', {}, 'Reports contain filenames, fingerprints, headings and finding text. Review them before sharing. Edits retire reports and downloads. Exact finding spans are unavailable; navigation opens the source chapter. ',
      node('a', { href: '../INSPECTION.md' }, 'Inspection scope and limits')));
  const publish = root.querySelector('[aria-labelledby="publish-title"]');
  if (publish) publish.before(panel); else root.body.append(panel);
  return panel;
}
