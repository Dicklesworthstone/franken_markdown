import assert from 'node:assert/strict';
import test from 'node:test';
import { createBookPdfControls, createBookPdfPanel } from '../demo/book_pdf_controls.mjs';
import { proofHost, pdfResult } from './book_pdf_proof_helpers.mjs';

// Small DOM/URL ownership doubles, not a browser plugin or native renderer.
class Element extends EventTarget {
  constructor(root, tag) {
    super(); Object.assign(this, { root, tagName: tag, attrs: {}, style: {}, children: [], disabled: false, hidden: false });
  }
  set id(value) { this._id = value; this.root.nodes[value] = this; }
  get id() { return this._id; }
  set innerHTML(_) { assert.fail('Proof UI must not parse HTML'); }
  appendChild(node) {
    if (this.failInsert) throw Error('insertion denied');
    this.children.push(node); node.parentElement = this; return node;
  }
  insertBefore(node, before) {
    this.children.splice(this.children.indexOf(before), 0, node); node.parentElement = this; return node;
  }
  replaceChildren(...children) { this.children = []; for (const node of children) this.appendChild(node); }
  setAttribute(name, value) { this.attrs[name] = String(value); }
  getAttribute(name) { return this.attrs[name] ?? null; }
  removeAttribute(name) { delete this.attrs[name]; delete this[name]; }
  emit(type) { const event = new Event(type, { cancelable: true }); this.dispatchEvent(event); return event; }
}
function fixture({ navigator = {}, failUrl = false } = {}) {
  const host = proofHost();
  const root = { nodes: {}, querySelector(selector) { return this.nodes[selector.slice(1)] ?? null; },
    createElement(tag) { return new Element(this, tag); } };
  const body = root.createElement('body'), section = root.createElement('section');
  body.appendChild(section);
  for (const id of ['publish-title', 'chapter-source', 'chapter-path', 'source-role', 'chapters', 'title', 'author', 'lang', 'font', 'dark-mode', 'font-scale', 'toc', 'page-numbers', 'download', 'preview-frame']) {
    const node = root.createElement(id === 'chapter-source' ? 'textarea' : 'input'); node.id = id; section.appendChild(node);
  }
  root.nodes.download.href = 'blob:unrelated-export';
  root.nodes['preview-frame'].srcdoc = 'unrelated HTML preview';
  const live = new Map(), revoked = [];
  let serial = 0;
  const urls = {
    createObjectURL(blob) { if (failUrl) throw Error('URL creation failed'); const url = `blob:proof/${++serial}`; live.set(url, blob); return url; },
    revokeObjectURL(url) {
      assert.notEqual(root.nodes['book-pdf-download'].href, url, 'detach before revoke');
      assert.equal(root.nodes['book-pdf-viewport'].children.length, 0, 'detach native viewer before revoke');
      live.delete(url); revoked.push(url);
    },
  };
  const controls = createBookPdfControls({ root, ...host, urls, navigator });
  return { host, root, body, el: root.nodes, controls, urls, live, revoked };
}
async function build(f, result = pdfResult()) {
  const promise = f.controls.build();
  f.host.jobs.at(-1).resolve(result);
  return promise;
}

test('installs one accessible panel beside publishing; no job starts automatically', () => {
  const f = fixture();
  assert.equal(createBookPdfPanel(f.root), f.el['book-pdf-proof']);
  assert.equal(f.body.children.length, 2);
  assert.equal(f.el['book-pdf-proof'].getAttribute('aria-labelledby'), 'book-pdf-title');
  assert.equal(f.el['book-pdf-status'].getAttribute('role'), 'status');
  assert.equal(f.el['book-pdf-build'].disabled, false);
  assert.equal(f.el['book-pdf-cancel'].disabled, true);
  assert.equal(f.el['book-pdf-download'].hidden, true);
  assert.equal(f.host.jobs.length, 0);
});

test('viewer and download share the very same Blob; repeated proof generation does not render again', async () => {
  const f = fixture(), proof = await build(f);
  const link = f.el['book-pdf-download'], viewer = f.el['book-pdf-viewer'];
  assert.equal(link.hidden, false);
  assert.equal(viewer.type, 'application/pdf');
  assert.equal(viewer.data, link.href);
  assert.equal(f.live.get(link.href), proof.blob);
  assert.equal(link.download, proof.filename);
  assert.equal(link.emit('click').defaultPrevented, false);
  assert.equal(f.host.jobs.length, 1);
  assert.equal(await f.controls.build(), proof);
  assert.equal(f.host.jobs.length, 1);
  assert.equal(f.live.size, 1);
  assert.match(f.el['book-pdf-status'].textContent, /Nothing has been saved yet/);
  assert.equal(f.el.download.href, 'blob:unrelated-export');
  assert.equal(f.el['preview-frame'].srcdoc, 'unrelated HTML preview');
});

test('no native PDF viewer still permits downloading exact proof bytes', async () => {
  const f = fixture({ navigator: { pdfViewerEnabled: false } });
  const proof = await build(f);
  assert.equal(f.el['book-pdf-viewport'].children.length, 0);
  assert.equal(f.el['book-pdf-download'].hidden, false);
  assert.equal(f.live.get(f.el['book-pdf-download'].href), proof.blob);
  assert.match(f.el['book-pdf-status'].textContent, /viewing is unavailable/);
});

test('failed native-viewer insertion degrades to download, without losing valid output', async () => {
  const f = fixture();
  f.el['book-pdf-viewport'].failInsert = true;
  const proof = await build(f);
  assert.equal(f.el['book-pdf-download'].hidden, false);
  assert.equal(f.live.get(f.el['book-pdf-download'].href), proof.blob);
  assert.equal(f.el['book-pdf-viewport'].children.length, 0);
});

test('URL failure exposes no half-installed output and retries the retained PDF without rerendering', async () => {
  const f = fixture({ failUrl: true });
  await assert.rejects(build(f), /URL creation failed/);
  assert.equal(f.el['book-pdf-download'].hidden, true);
  assert.equal(f.live.size, 0);
  f.urls.createObjectURL = blob => { f.live.set('blob:retry', blob); return 'blob:retry'; };
  await f.controls.build();
  assert.equal(f.host.jobs.length, 1);
  assert.equal(f.el['book-pdf-download'].hidden, false);
});

test('raw input events immediately detach viewer and download while leaving ordinary exports alone', async () => {
  for (const id of ['chapter-source', 'source-role', 'chapters', 'font-scale', 'toc', 'page-numbers']) {
    const f = fixture(); await build(f);
    const old = f.el['book-pdf-download'].href;
    f.el[id].emit('input');
    assert.equal(f.el['book-pdf-download'].hidden, true);
    assert.deepEqual(f.revoked, [old]);
    assert.equal(f.el['book-pdf-viewport'].children.length, 0);
    assert.equal(f.el.download.href, 'blob:unrelated-export');
  }
});

test('silent edits block click, middle click and context-menu download activation', async () => {
  for (const type of ['click', 'auxclick', 'contextmenu']) {
    const f = fixture(); await build(f);
    f.host.rawEdit('unreported editor mutation');
    assert.equal(f.el['book-pdf-download'].emit(type).defaultPrevented, true);
    assert.equal(f.live.size, 0);
    assert.equal(f.el['book-pdf-download'].hidden, true);
  }
});

test('resource changes retire the proof and focus rechecks silent source changes', async () => {
  const f = fixture(); await build(f);
  f.host.change(state => { state.options.images = []; });
  assert.equal(f.live.size, 0);
  await build(f);
  f.host.rawEdit('changed silently');
  f.el['book-pdf-proof'].emit('focusin');
  assert.equal(f.live.size, 0);
  assert.equal(f.el['book-pdf-download'].hidden, true);
});

test('busy state and cancel retire only the proof, then allow explicit retry', async () => {
  const f = fixture();
  const pending = f.controls.build();
  assert.equal(f.el['book-pdf-build'].disabled, true);
  assert.equal(f.el['book-pdf-cancel'].disabled, false);
  assert.equal(f.el['book-pdf-proof'].getAttribute('aria-busy'), 'true');
  f.el['book-pdf-cancel'].emit('click');
  await assert.rejects(pending, { code: 'EXPORT_CANCELLED' });
  assert.equal(f.host.jobs[0].signal.aborted, true);
  assert.equal(f.el['book-pdf-build'].disabled, false);
  assert.equal(f.el.download.href, 'blob:unrelated-export');
  f.host.jobs[0].reject(Error('late'));
  await build(f);
  f.el['book-pdf-release'].emit('click');
  assert.equal(f.live.size, 0);
  assert.equal(f.el['book-pdf-download'].hidden, true);
});

test('composition and import block work; cancelled late results cannot overwrite status or a later proof', async () => {
  const f = fixture();
  const old = f.controls.build();
  f.el['chapter-source'].emit('compositionstart');
  await assert.rejects(old, { code: 'STALE_SOURCE' });
  await assert.rejects(f.controls.build(), { code: 'BOOK_BUSY' });
  f.el['chapter-source'].emit('compositionend');
  const proof = await build(f);
  f.host.jobs[0].resolve(pdfResult());
  await new Promise(resolve => setTimeout(resolve, 0));
  assert.equal(f.live.get(f.el['book-pdf-download'].href), proof.blob);
  assert.match(f.el['book-pdf-status'].textContent, /PDF generated/);
  f.host.busy(true);
  assert.equal(f.live.size, 0);
  await assert.rejects(f.controls.build(), { code: 'BOOK_BUSY' });
});

test('suspension releases native viewing, return does not rerender and disposal removes listeners', async () => {
  const f = fixture(); await build(f);
  f.controls.suspend();
  assert.equal(f.live.size, 0);
  assert.equal(f.el['book-pdf-build'].disabled, true);
  f.controls.resume();
  assert.equal(f.host.jobs.length, 1);
  assert.equal(f.el['book-pdf-build'].disabled, false);
  const pending = f.controls.build();
  f.controls.dispose();
  await assert.rejects(pending, { code: 'PDF_PROOF_CLOSED' });
  f.controls.dispose();
  assert.equal(f.host.disposed, 1);
  assert.equal(f.host.listenerCount, 0);
  f.el['book-pdf-build'].emit('click');
  assert.equal(f.host.jobs.length, 2);
});

test('empty collection disables proofing and invalid output cannot activate a download', async () => {
  const f = fixture();
  f.host.change(state => { state.files = []; });
  assert.equal(f.el['book-pdf-build'].disabled, true);
  await assert.rejects(f.controls.build(), { code: 'EMPTY_BOOK' });
  assert.equal(f.live.size, 0);
  const g = fixture();
  await assert.rejects(build(g, pdfResult(new TextEncoder().encode('<script>bad'))), { code: 'INVALID_BOOK_PDF' });
  assert.equal(g.live.size, 0);
  assert.equal(g.el['book-pdf-download'].hidden, true);
  assert.match(g.el['book-pdf-status'].textContent, /INVALID_BOOK_PDF/);
});
