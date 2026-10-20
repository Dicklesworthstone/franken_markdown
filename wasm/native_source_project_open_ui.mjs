// Embedded, import-free Open Project controls. Reopening creates a separate
// workspace: never replace the current editor, its history, or its resources.
export function installNativeSourceProjectOpen(createPublisher, parts, runtime, host, doc = document) {
  if (!runtime || typeof host?.ready !== 'function' || typeof parts.createNativeSourceProjectOpenWorker !== 'function') return null;
  let controls = null, disposed = false;
  function install() {
    if (disposed || controls) return;
    const win = doc.defaultView, header = doc.querySelector('body > .fmd-app-header');
    const save = header?.querySelector('#btn-save-html');
    const payload = doc.querySelector('body > script#fmd-native-runtime[type="application/json"]');
    const editor = doc.querySelector('body > #fmd-app-body > #editor-pane > textarea#fmd-editor');
    if (!save || !payload || !editor) return;
    header.querySelector('#btn-open-source-project')?.remove();
    header.querySelector('#fmd-project-open-notice')?.remove();
    doc.querySelector('body > dialog#fmd-open-source-project')?.remove();
    doc.querySelector('head > style#fmd-project-open-style')?.remove();
    const make = (tag, id, text, parent) => {
      const node = doc.createElement(tag); if (id) node.id = id;
      if (text !== undefined) node.textContent = text;
      if (parent) parent.appendChild(node); return node;
    };
    const toggle = make('button', 'btn-open-source-project', 'Open source project');
    toggle.type = 'button'; toggle.className = 'fmd-btn';
    toggle.setAttribute('aria-haspopup', 'dialog'); toggle.setAttribute('aria-controls', 'fmd-open-source-project');
    save.parentNode.insertBefore(toggle, save);
    const notice = make('span', 'fmd-project-open-notice', '', header); notice.setAttribute('role', 'status');
    const style = make('style', 'fmd-project-open-style', '@media print { #fmd-open-source-project { display:none !important; } }', doc.head);
    const dialog = make('dialog', 'fmd-open-source-project', undefined, doc.body);
    dialog.setAttribute('aria-labelledby', 'fmd-project-open-title'); dialog.setAttribute('aria-describedby', 'fmd-project-open-help');
    dialog.style.cssText = 'width:min(94vw,44rem);max-height:92vh;overflow:auto;box-sizing:border-box;margin:auto;padding:20px;border:1px solid var(--border-color,#aaa);border-radius:8px;background:var(--bg-primary,#fff);color:var(--fg-primary,#222)';
    make('h2', 'fmd-project-open-title', 'Open a source project', dialog);
    make('p', 'fmd-project-open-help', 'Choose a .fmd.json file and build a new editable HTML workspace with this document’s bundled engine. Download the result and open that HTML file. Your current document, unsaved edits and resources are not replaced.', dialog);
    const label = make('label', '', 'Local source project (.fmd.json)', dialog);
    label.htmlFor = 'fmd-project-open-file'; label.style.display = 'block';
    const input = make('input', 'fmd-project-open-file', undefined, dialog);
    input.type = 'file'; input.accept = '.fmd.json,.json,application/json'; input.multiple = false;
    input.style.cssText = 'display:block;width:100%;max-width:100%;box-sizing:border-box;margin:12px 0';
    const row = make('div', '', undefined, dialog); row.style.cssText = 'display:flex;flex-wrap:wrap;gap:10px;margin:12px 0';
    const button = (id, text) => { const node = make('button', id, text, row); node.type = 'button'; node.className = 'fmd-btn'; return node; };
    const build = button('btn-build-source-project', 'Build workspace');
    const cancel = button('btn-cancel-open-project', 'Cancel opening');
    const download = button('btn-download-opened-project', 'Download workspace');
    const close = button('btn-close-open-project', 'Close');
    const status = make('p', 'fmd-project-open-status', 'Choose a project, then select Build workspace. Nothing is uploaded.', dialog);
    status.setAttribute('role', 'status'); status.setAttribute('aria-live', 'polite'); status.style.overflowWrap = 'anywhere';
    const findings = make('ul', 'fmd-project-open-diagnostics', undefined, dialog);
    let alive = true, suspended = false, composing = false, pending = null, prepared = null, url = null;
    const listeners = [];
    const listen = (target, kind, fn) => { target.addEventListener(kind, fn); listeners.push([target, kind, fn]); };
    function available() { try { return alive && !suspended && !composing && host.ready(); } catch { return false; } }
    function selected() { return input.files?.length === 1 ? input.files[0] : null; }
    function busy() { return Boolean(runtime.settingsPending || runtime.exportPending); }
    function matches(snapshot) {
      return available() && dialog.open && selected() === snapshot.file && payload.textContent === snapshot.runtime;
    }
    function refresh() {
      toggle.disabled = !available(); input.disabled = !available();
      build.disabled = !available() || !dialog.open || !selected() || pending !== null || busy();
      cancel.disabled = pending === null && prepared === null;
      download.disabled = !prepared || !matches(prepared);
      dialog.setAttribute('aria-busy', String(pending !== null));
    }
    function revoke() { if (url !== null) win.URL.revokeObjectURL(url); url = null; }
    function invalidate(message) {
      const job = pending; pending = null; job?.operation?.cancel();
      prepared = null; revoke(); findings.replaceChildren();
      if (message) status.textContent = message;
      refresh();
    }
    function start() {
      if (!available() || !dialog.open || pending) return false;
      if (busy()) { status.textContent = 'Finish or cancel the current export or settings operation, then retry.'; refresh(); return false; }
      const file = selected(); if (!file) return false;
      invalidate();
      const job = {file, runtime: payload.textContent, operation: null}; pending = job;
      status.textContent = 'Reading, validating and rendering the selected project in a background worker…'; refresh();
      try {
        job.operation = parts.createNativeSourceProjectOpenWorker(createPublisher, parts, {file, runtime: job.runtime}, win);
        if (pending !== job || !matches(job)) { job.operation.cancel(); if (pending === job) invalidate('Selection or workspace changed; build again.'); return false; }
        job.operation.promise.then(value => {
          if (pending !== job) return;
          if (!matches(job)) { invalidate('Selection or workspace changed; build again.'); return; }
          try {
            const blob = new Blob([value.bytes], {type: 'text/html;charset=utf-8'});
            if (pending !== job || !matches(job)) { if (pending === job) invalidate('Selection or workspace changed; build again.'); return; }
            prepared = {file, runtime: job.runtime, blob, filename: value.filename}; pending = null;
            for (const message of value.diagnostics.slice(0, 20)) make('li', '', message, findings);
            status.textContent = 'Workspace ready: ' + blob.size.toLocaleString() + ' bytes. Download it and open the HTML file; your current document is unchanged.'
              + (value.diagnosticCount ? ' Renderer diagnostics: ' + value.diagnosticCount + (value.diagnosticCount > 20 ? ' (showing first 20).' : '.') : '');
            refresh();
          } catch (error) { invalidate('Unable to prepare workspace: ' + String(error?.message ?? error).slice(0, 2048)); }
        }, error => {
          if (pending !== job) return;
          invalidate('Project opening failed: ' + String(error?.message ?? error).slice(0, 2048) + ' — current document unchanged; retry explicitly.');
        });
        return true;
      } catch (error) {
        invalidate('Project opening failed: ' + String(error?.message ?? error).slice(0, 2048) + ' — current document unchanged.'); return false;
      }
    }
    function downloadPrepared() {
      const owned = prepared;
      if (!owned || !matches(owned)) { invalidate('Selection or workspace changed; build again before downloading.'); return false; }
      revoke(); let next = null, link = null;
      try {
        next = win.URL.createObjectURL(owned.blob);
        if (prepared !== owned || !matches(owned)) { win.URL.revokeObjectURL(next); next = null; invalidate('Selection or workspace changed; build again.'); return false; }
        url = next; next = null;
        link = make('a'); link.href = url; link.download = owned.filename; doc.body.appendChild(link);
        if (prepared !== owned || !matches(owned)) { invalidate('Selection or workspace changed; build again.'); return false; }
        link.click(); status.textContent = 'Workspace download requested. Open the downloaded HTML file to edit the project; this document remains unchanged.';
        return true;
      } catch (error) {
        if (next !== null) win.URL.revokeObjectURL(next); revoke();
        status.textContent = 'Workspace download failed: ' + String(error?.message ?? error).slice(0, 2048) + ' — retry the download explicitly.';
        return false;
      } finally { link?.remove(); refresh(); }
    }
    function open() {
      if (!available() || dialog.open) return false;
      try { dialog.showModal(); notice.textContent = ''; refresh(); input.focus(); return true; }
      catch (error) { notice.textContent = 'Unable to open project controls: ' + String(error?.message ?? error).slice(0, 256); return false; }
    }
    function dismiss() {
      invalidate('Project opening closed; current document unchanged.'); input.value = '';
      if (dialog.open) dialog.close(); if (available()) toggle.focus();
    }
    listen(toggle, 'click', open); listen(build, 'click', start); listen(download, 'click', downloadPrepared);
    listen(cancel, 'click', () => invalidate('Project opening cancelled; current document unchanged.'));
    listen(close, 'click', dismiss);
    // File inputs also emit bubbling cancel events (including reselecting the
    // same file). Only the dialog's own Escape/close request should dismiss it.
    listen(dialog, 'cancel', event => { if (event.target === dialog) { event.preventDefault(); dismiss(); } });
    listen(dialog, 'close', () => { if (!dialog.open) { invalidate('Project opening closed; current document unchanged.'); input.value = ''; } });
    listen(input, 'change', () => invalidate('Selection changed. Select Build workspace to open this project.'));
    listen(dialog, 'focusin', refresh);
    listen(editor, 'compositionstart', () => { composing = true; invalidate('Text composition started; project opening cancelled.'); });
    listen(editor, 'compositionend', () => { composing = false; refresh(); });
    listen(win, 'pagehide', () => {
      suspended = true; composing = false; invalidate('Workspace suspended; project opening released.'); input.value = '';
      if (dialog.open) dialog.close();
    });
    listen(win, 'pageshow', () => { suspended = false; composing = false; refresh(); });
    listen(win, 'fmd-native-ready', refresh);
    const observer = new win.MutationObserver(() => invalidate('Current workspace resources changed; build the selected project again.'));
    observer.observe(payload, {childList: true, characterData: true, subtree: true});
    controls = {open, dispose() {
      alive = false; invalidate(); input.value = ''; observer.disconnect();
      for (const [target, kind, fn] of listeners) target.removeEventListener(kind, fn);
      toggle.remove(); notice.remove(); dialog.remove(); style.remove();
    }};
    runtime.ready?.then(refresh, refresh); refresh();
  }
  if (doc.readyState === 'loading') doc.addEventListener('DOMContentLoaded', install, {once: true}); else install();
  return Object.freeze({open: () => controls?.open() ?? false, dispose() {
    disposed = true; doc.removeEventListener('DOMContentLoaded', install); controls?.dispose();
  }});
}
