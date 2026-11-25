// Embedded in exported workspaces. The shadow tree keeps the origin's recovery
// catalog, validation results and Blob links out of serialized workspace HTML.
export function installNativeRecovery(parts, host, doc = document) {
  let cleanup, disposed = false;
  function install() {
    if (disposed || cleanup) return;
    const win = doc.defaultView;
    const header = doc.querySelector('body > .fmd-app-header');
    const save = header?.querySelector('#btn-save-html');
    const editor = doc.querySelector('body > #fmd-app-body > #editor-pane > textarea#fmd-editor');
    const payload = doc.querySelector('body > script#fmd-native-runtime[type="application/json"]');
    if (!save || !editor || !payload) return;

    const container = doc.createElement('span');
    if (typeof container.attachShadow !== 'function') return;
    header.querySelector('#fmd-local-recovery')?.remove();
    container.id = 'fmd-local-recovery';
    const root = container.attachShadow({mode: 'open'});
    save.parentNode.insertBefore(container, save);
    function element(tag, parent, text) {
      const node = doc.createElement(tag);
      if (text !== undefined) node.textContent = text;
      parent.appendChild(node);
      return node;
    }
    function button(parent, label, action) {
      const node = element('button', parent, label);
      node.type = 'button';
      node.addEventListener('click', action);
      return node;
    }
    element('style', root, ':host{display:inline-flex}button,a{font:inherit}button{cursor:pointer}dialog{width:min(38rem,88vw);max-height:80vh;overflow:auto;box-sizing:border-box;border:1px solid #888;border-radius:8px;padding:1.2rem;background:var(--bg-primary,Canvas);color:var(--fg-primary,CanvasText)}dialog::backdrop{background:#0006}li,p{overflow-wrap:anywhere}button,a{margin:.2em} @media print{:host{display:none}}');
    const toggle = button(root, 'Local recovery', () => {
      if (!disposed && !suspended && !dialog.open) { dialog.showModal(); enable.focus(); }
    });
    toggle.setAttribute('aria-haspopup', 'dialog');
    const dialog = element('dialog', root);
    element('h2', dialog, 'Local recovery').id = 'recovery-title';
    dialog.setAttribute('aria-labelledby', 'recovery-title');
    element('p', dialog, 'Store an editable copy in this browser after changes. Copies include Markdown, committed settings, images and fonts. Workspaces on the same origin can access them. Keep a downloaded backup.');
    const enable = button(dialog, 'Enable autosave', () => {
      if (session.state.enabled) session.disable();
      else session.enable();
    });
    const saveNow = button(dialog, 'Save now', () => session.flush());
    const writerStatus = element('p', dialog);
    writerStatus.setAttribute('role', 'status');
    button(dialog, 'Show saved copies', list);
    const catalogStatus = element('p', dialog);
    catalogStatus.setAttribute('role', 'status');
    const rows = element('ul', dialog);
    const download = element('a', dialog, 'Download recovered project');
    download.hidden = true;
    button(dialog, 'Close', () => dialog.close());
    let store, suspended = false, composing = false, generation = 0, validation, downloadUrl;
    const encoder = parts.createNativeRecoveryEncoder(parts.createNativeSourceProjectCodec, win);
    function storage() {
      // Some origins throw even on reading window.indexedDB. Access it only
      // in response to a storage operation, inside the caller's error boundary.
      return store ??= parts.createNativeRecoveryStore({indexedDB: win.indexedDB});
    }
    function ready() {
      try { return !disposed && !suspended && !composing && host.ready(); }
      catch { return false; }
    }
    function refresh(state) {
      enable.textContent = state.enabled ? 'Turn off autosave' : 'Enable autosave';
      enable.disabled = !state.enabled && !ready();
      saveNow.disabled = !state.enabled || !state.dirty || state.busy || !ready();
      writerStatus.textContent = state.message;
      toggle.textContent = state.enabled ? 'Local recovery on'
        : state.message.startsWith('Local recovery stopped:') ? 'Local recovery stopped' : 'Local recovery';
    }
    const session = parts.createNativeRecoverySession({
      store: {write: project => storage().write(project)}, encoder,
      ready, capture: () => ({source: host.source(), runtime: payload.textContent}), notify: refresh,
    });
    function resetBrowse(clearRows = true) {
      generation++;
      validation?.cancel(); validation = null;
      if (downloadUrl) win.URL.revokeObjectURL(downloadUrl);
      downloadUrl = null;
      download.hidden = true; download.removeAttribute('href'); download.removeAttribute('download');
      if (clearRows) rows.replaceChildren();
      catalogStatus.textContent = '';
    }
    function current(token) { return !disposed && !suspended && dialog.open && token === generation; }
    function failure(token, error) {
      if (current(token)) catalogStatus.textContent = String(error?.message ?? error).slice(0, 512);
    }
    async function prepare(record) {
      if (!current(generation)) return;
      resetBrowse(false);
      const token = generation;
      catalogStatus.textContent = 'Preparing recovered project…';
      try {
        const saved = await storage().read(record.id, record.revision);
        if (!current(token)) return;
        validation = encoder.validate(saved.blob);
        const project = await validation.promise;
        if (!current(token)) return;
        validation = null;
        downloadUrl = win.URL.createObjectURL(project.blob);
        download.href = downloadUrl; download.download = project.filename; download.hidden = false;
        catalogStatus.textContent = 'Download the project, then use Open source project to rebuild an editable workspace.';
        download.focus();
      } catch (error) { failure(token, error); }
    }
    async function remove(record) {
      if (!current(generation) || !win.confirm('Delete the stored recovery copy of “' + record.title + '” (revision ' + record.revision + ')?')) return;
      resetBrowse(false);
      const token = generation;
      try {
        await storage().remove(record.id, record.revision);
        session.forget(record.id);
        if (current(token)) await list();
      } catch (error) { failure(token, error); }
    }
    async function list() {
      if (!current(generation)) return;
      resetBrowse();
      const token = generation;
      catalogStatus.textContent = 'Reading saved copies…';
      try {
        const records = await storage().list();
        if (!current(token)) return;
        catalogStatus.textContent = records.length ? 'Choose a saved revision.' : 'No local recovery copies.';
        for (const record of records) {
          const row = element('li', rows);
          element('span', row, record.title + ' · revision ' + record.revision + ' · ' + new Date(record.updatedAt).toLocaleString());
          button(row, 'Prepare download', () => prepare(record));
          button(row, 'Delete copy', () => remove(record));
        }
      } catch (error) { failure(token, error); }
    }
    dialog.addEventListener('close', () => { if (!dialog.open) resetBrowse(); });
    const listeners = [];
    function listen(target, event, fn) {
      target.addEventListener(event, fn); listeners.push([target, event, fn]);
    }
    function changed() {
      if (session.state.enabled) session.changed();
      else refresh(session.state);
    }
    listen(editor, 'input', changed);
    listen(editor, 'compositionstart', () => { composing = true; changed(); });
    listen(editor, 'compositionend', () => { composing = false; changed(); });
    const observer = new win.MutationObserver(changed);
    observer.observe(payload, {childList: true, characterData: true, subtree: true});
    listen(win, 'pagehide', () => {
      suspended = true; composing = false; session.suspend(); resetBrowse();
      if (dialog.open) dialog.close();
    });
    listen(win, 'pageshow', () => { suspended = false; refresh(session.state); });
    cleanup = () => {
      session.dispose(); resetBrowse(); observer.disconnect();
      for (const [target, event, fn] of listeners) target.removeEventListener(event, fn);
      container.remove();
    };
    refresh(session.state);
  }
  if (doc.readyState === 'loading') doc.addEventListener('DOMContentLoaded', install, {once: true});
  else install();
  return Object.freeze({dispose() {
    disposed = true; doc.removeEventListener('DOMContentLoaded', install); cleanup?.();
  }});
}
