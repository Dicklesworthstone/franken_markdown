// Serialized into native workspaces before the editor controller. Keep this
// registration and its closures import-free; all source access belongs to the
// controller's lossless source anchor, never a second textarea interpretation.
export function registerNativePdfProof(createSession, runtime) {
  if (!runtime || typeof runtime.beginExportDocument !== 'function') return;
  let installed = false;
  Object.defineProperty(runtime, 'installPdfProof', {value(host) {
    if (installed) return;
    if (typeof host?.source !== 'function' || typeof host.download !== 'function'
        || typeof host.ready !== 'function') throw new TypeError('PDF proofing requires a lossless source host');
    installed = true;
    function install() {
      const doc = document, win = doc.defaultView;
      const header = doc.querySelector('body > .fmd-app-header');
      const save = header?.querySelector('#btn-save-html');
      const editor = doc.querySelector('body > #fmd-app-body > #editor-pane > textarea#fmd-editor');
      const payload = doc.querySelector('body > script#fmd-native-runtime[type="application/json"]');
      if (!save || !editor || !payload) return;
      // A saved shell's transient controls are never authoritative. Parsing is
      // complete before reconstruction, including files saved by older hosts.
      header.querySelector('#btn-pdf-proof')?.remove();
      doc.querySelector('body > dialog#fmd-pdf-proof')?.remove();
      doc.querySelector('head > style#fmd-pdf-proof-style')?.remove();
      const make = (tag, text, parent) => {
        const node = doc.createElement(tag);
        if (text !== undefined) node.textContent = text;
        if (parent) parent.appendChild(node);
        return node;
      };
      const toggle = make('button', 'PDF proof'); toggle.type = 'button';
      toggle.id = 'btn-pdf-proof'; toggle.className = 'fmd-btn';
      toggle.setAttribute('aria-haspopup', 'dialog'); toggle.setAttribute('aria-controls', 'fmd-pdf-proof');
      save.parentNode.insertBefore(toggle, save);
      const style = make('style', '@media print { #fmd-pdf-proof { display:none !important; } }', doc.head);
      style.id = 'fmd-pdf-proof-style';
      const dialog = make('dialog', undefined, doc.body); dialog.id = 'fmd-pdf-proof';
      dialog.setAttribute('aria-labelledby', 'fmd-proof-title'); dialog.setAttribute('aria-describedby', 'fmd-proof-help');
      dialog.style.cssText = 'width:min(96vw,76rem);max-height:94vh;overflow:auto;box-sizing:border-box;margin:auto;padding:20px;border:1px solid var(--border-color,#aaa);border-radius:8px;background:var(--bg-primary,#fff);color:var(--fg-primary,#222)';
      make('h2', 'Native PDF proof', dialog).id = 'fmd-proof-title';
      make('p', 'Generate the actual PDF using committed paper, margins, typography, images and fonts. Viewing zoom and unapplied settings drafts do not affect it. Download this PDF keeps these exact bytes without rendering again.', dialog).id = 'fmd-proof-help';
      const controls = make('div', undefined, dialog);
      controls.style.cssText = 'display:flex;flex-wrap:wrap;gap:10px;margin:12px 0';
      const button = (id, text) => {
        const node = make('button', text, controls); node.id = id; node.type = 'button'; node.className = 'fmd-btn'; return node;
      };
      const generate = button('btn-generate-pdf-proof', 'Generate PDF proof');
      const cancel = button('btn-cancel-pdf-proof', 'Cancel rendering');
      const download = button('btn-download-pdf-proof', 'Download this PDF');
      const close = button('btn-close-pdf-proof', 'Close');
      const status = make('p', 'Generate a proof to inspect the native PDF, not the HTML preview.', dialog);
      status.id = 'fmd-proof-status'; status.setAttribute('role', 'status'); status.setAttribute('aria-live', 'polite');
      status.style.cssText = 'margin:10px 0;overflow-wrap:anywhere';
      const help = make('p', 'Embedded PDF viewing depends on your browser. When unavailable or blank, use Download this PDF and inspect it in your PDF viewer. No external viewer or network service is loaded.', dialog);
      help.id = 'fmd-proof-viewer-help';
      const findings = make('ul', undefined, dialog); findings.id = 'fmd-proof-diagnostics';
      const viewport = make('div', undefined, dialog); viewport.id = 'fmd-proof-viewport';
      let suspended = false, composing = false, operation = null, shown = null, objectUrl = null;
      const ready = () => !suspended && !composing && host.ready() && runtime.settings !== null;
      const session = createSession(runtime, () => {
        if (!ready()) throw Error('PDF proofing is paused; finish composition or return to the document');
        return {source: host.source(), settings: runtime.settings, resources: runtime.documentRevision};
      });
      function releaseView() {
        viewport.replaceChildren(); findings.replaceChildren(); shown = null;
        if (objectUrl !== null) { win.URL.revokeObjectURL(objectUrl); objectUrl = null; }
      }
      function currentProof() {
        try { return ready() ? session.current : null; }
        catch { return null; }
      }
      function refresh() {
        const current = currentProof();
        if (shown && shown !== current) {
          releaseView(); status.textContent = 'Document changed; generate a new PDF proof.';
        }
        toggle.disabled = suspended || runtime.settings === null;
        // Other native operations may finish without a DOM event. Keep retry
        // available: the owned export API rejects busy admission synchronously.
        generate.disabled = !ready() || operation !== null;
        cancel.disabled = operation === null;
        download.disabled = !ready() || !current || shown !== current;
        dialog.setAttribute('aria-busy', String(operation !== null));
      }
      function invalidate(message) {
        operation = null; session.invalidate(message); releaseView();
        if (message && dialog.open) status.textContent = message;
        refresh();
      }
      function show(proof) {
        // URL creation and DOM installation are transactional too: failure leaves
        // no leaked URL or download button suggesting an unavailable proof.
        releaseView();
        let url = null;
        try {
          if (win.navigator.pdfViewerEnabled !== false) {
            url = win.URL.createObjectURL(proof.blob);
            const viewer = make('object'); viewer.type = 'application/pdf';
            viewer.id = 'fmd-proof-viewer'; viewer.setAttribute('aria-label', 'Generated native PDF proof');
            viewer.style.cssText = 'display:block;width:100%;height:65vh;min-height:320px;border:1px solid var(--border-color,#aaa);margin-top:12px';
            viewer.data = url;
            make('p', 'Embedded PDF viewer unavailable. Use Download this PDF above.', viewer);
            viewport.appendChild(viewer);
          }
          for (const message of proof.diagnostics.slice(0, 100)) make('li', message.slice(0, 2000), findings);
          if (proof.diagnostics.length > 100) make('li', 'Showing the first 100 of ' + proof.diagnostics.length + ' renderer diagnostics.', findings);
          objectUrl = url; url = null; shown = proof;
          status.textContent = 'PDF generated: ' + proof.size.toLocaleString() + ' bytes. Download this PDF preserves exactly this output.'
            + (proof.diagnostics.length ? ' Renderer diagnostics: ' + proof.diagnostics.length + '.' : '');
          // A browser plugin load event does not prove that page content rendered.
          // Never claim page counts, visual fidelity or accessibility certification.
        } catch (error) {
          if (url !== null) win.URL.revokeObjectURL(url);
          releaseView(); throw error;
        }
      }
      async function render() {
        if (operation || !dialog.open || !ready()) return;
        const owned = {}; operation = owned;
        status.textContent = 'Generating native PDF in background…'; refresh();
        try {
          const proof = await session.render();
          if (operation !== owned || !dialog.open || !ready() || session.current !== proof) return;
          show(proof);
        } catch (error) {
          if (operation === owned) status.textContent = 'PDF proof failed: ' + String(error?.message ?? error).slice(0, 2048)
            + ' — document unchanged; retry explicitly.';
        } finally {
          if (operation === owned) operation = null;
          refresh();
        }
      }
      toggle.addEventListener('click', () => {
        if (toggle.disabled || dialog.open) return;
        dialog.showModal(); refresh(); generate.focus();
      });
      generate.addEventListener('click', () => { render(); });
      cancel.addEventListener('click', () => {
        operation = null; session.cancel();
        status.textContent = 'PDF proofing cancelled; document is unchanged.'; refresh();
      });
      download.addEventListener('click', () => {
        try {
          const proof = currentProof();
          if (!proof || shown !== proof) { invalidate('Document changed; generate a new proof before downloading.'); return; }
          host.download(proof.blob);
          status.textContent = 'PDF download started using the proof bytes — check your downloads.';
        } catch (error) {
          status.textContent = 'Unable to download proof: ' + String(error?.message ?? error).slice(0, 2048);
        }
        refresh();
      });
      function dismiss() {
        invalidate('Proof closed; generate again when ready.');
        if (dialog.open) dialog.close();
        toggle.focus();
      }
      close.addEventListener('click', dismiss);
      dialog.addEventListener('cancel', event => { event.preventDefault(); dismiss(); });
      dialog.addEventListener('close', () => {
        if (!dialog.open) invalidate('Proof closed; generate again when ready.');
      });
      editor.addEventListener('input', () => invalidate('Source changed; generate a new PDF proof.'));
      editor.addEventListener('compositionstart', () => { composing = true; invalidate('Text composition started; generate a proof when finished.'); });
      editor.addEventListener('compositionend', () => { composing = false; refresh(); });
      // Committed settings, image and font changes publish inert runtime data.
      // Observe just that application-owned block, never Markdown-controlled DOM.
      const observer = new win.MutationObserver(() => invalidate('Document resources or settings changed; generate a new PDF proof.'));
      observer.observe(payload, {childList: true, characterData: true, subtree: true});
      win.addEventListener('pagehide', () => { suspended = true; composing = false; invalidate('Workspace suspended; proof released.'); });
      win.addEventListener('pageshow', () => { suspended = false; composing = false; refresh(); });
      win.addEventListener('fmd-native-ready', refresh);
      // Refresh shared-lane admission after other toolbar operations complete;
      // no polling, automatic renders, or application-global monkeypatches.
      dialog.addEventListener('focusin', refresh);
      nativeReady();
      function nativeReady() {
        refresh();
        if (runtime.ready) runtime.ready.then(refresh, () => {});
      }
    }
    if (document.readyState === 'loading') document.addEventListener('DOMContentLoaded', install, {once: true});
    else install();
  }});
}
