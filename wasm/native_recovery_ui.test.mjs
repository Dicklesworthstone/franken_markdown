import assert from 'node:assert/strict';
import {test} from 'node:test';
import {installNativeRecovery} from './native_recovery_ui.mjs';
import {createNativeRecoverySession} from './native_recovery_session.mjs';

// Exercise the serialized installer and actual session. DOM events, the
// transactional store, worker and timer queue are controlled host adapters.
class Element {
  constructor(tag = '') {
    this.tag = tag; this.children = []; this.events = new Map(); this.textContent = '';
  }
  appendChild(node) { node.parentNode = this; this.children.push(node); return node; }
  insertBefore(node, before) { node.parentNode = this; this.children.splice(this.children.indexOf(before), 0, node); }
  replaceChildren() { this.children = []; }
  remove() { this.parentNode.children = this.parentNode.children.filter(child => child !== this); }
  attachShadow() { return this.shadowRoot = new Element(); }
  querySelector(selector) { return this.children.find(node => '#' + node.id === selector); }
  addEventListener(event, fn) {
    if (!this.events.has(event)) this.events.set(event, new Set());
    this.events.get(event).add(fn);
  }
  removeEventListener(event, fn) { this.events.get(event)?.delete(fn); }
  emit(event) { for (const fn of this.events.get(event) ?? []) fn(); }
  setAttribute(name, value) { this[name] = value; }
  removeAttribute(name) { delete this[name]; }
  showModal() { this.open = true; }
  close() { this.open = false; this.emit('close'); }
  focus() {}
}
function find(node, predicate) {
  if (predicate(node)) return node;
  for (const child of node.children) { const match = find(child, predicate); if (match) return match; }
}
function deferred() {
  let resolve, reject;
  const promise = new Promise((yes, no) => { resolve = yes; reject = no; });
  return {promise, resolve, reject};
}
function fixture({blocked = false} = {}) {
  const h = {access: 0, snapshots: [], writes: [], reads: [], removed: [], revoked: [], timers: new Set(),
    source: '\ufeff# exact\r\n', ready: true, confirmed: false, records: []};
  const doc = new Element(), win = h.win = new Element(), header = new Element();
  const save = header.appendChild(new Element()); save.id = 'btn-save-html';
  const editor = h.editor = new Element(), payload = h.payload = new Element();
  editor.value = '# normalized\n'; payload.textContent = 'committed settings and resources';
  doc.defaultView = win; doc.createElement = tag => new Element(tag);
  doc.querySelector = selector => ({
    'body > .fmd-app-header': header,
    'body > #fmd-app-body > #editor-pane > textarea#fmd-editor': editor,
    'body > script#fmd-native-runtime[type="application/json"]': payload,
  })[selector];
  Object.defineProperty(win, 'indexedDB', {get() { h.access++; if (blocked) throw Error('storage denied'); return {}; }});
  win.MutationObserver = class {
    constructor(callback) { h.mutate = callback; }
    observe() {}
    disconnect() { h.disconnected = true; }
  };
  win.confirm = () => h.confirmed;
  win.URL = {createObjectURL(blob) { h.download = blob; return 'blob:recovery'; }, revokeObjectURL(url) { h.revoked.push(url); }};
  const parts = {
    createNativeRecoverySession: options => createNativeRecoverySession({...options, randomId: () => 'a'.repeat(32),
      schedule: fn => { h.timers.add(fn); return fn; }, unschedule: fn => h.timers.delete(fn)}),
    createNativeRecoveryStore: () => ({
      write: async record => { h.writes.push(record); return {...record, revision: record.expectedRevision + 1, updatedAt: 1}; },
      list: () => h.listing?.promise ?? Promise.resolve(h.records),
      read: async (id, revision) => { h.reads.push([id, revision]); return {blob: 'stored bytes'}; },
      remove: async (id, revision) => { if (h.conflict) throw Error('revision changed'); h.removed.push([id, revision]); h.records = []; },
    }),
    createNativeRecoveryEncoder: () => ({
      encode(source, runtime) { h.snapshots.push({source, runtime}); return {promise: Promise.resolve({blob: 'encoded bytes'})}; },
      validate(blob) { h.validated = blob; return {promise: h.validation.promise, cancel() { h.cancelled = true; }}; },
    }),
  };
  h.api = Function('return (' + installNativeRecovery.toString() + ')')()(parts,
    {source: () => h.source, ready: () => h.ready}, doc);
  h.container = header.querySelector('#fmd-local-recovery');
  h.root = h.container.shadowRoot;
  h.byText = text => find(h.root, node => node.textContent === text);
  h.click = label => { const node = h.byText(label); assert.ok(node, label); assert.ok(!node.disabled, label); node.emit('click'); };
  h.open = () => h.click('Local recovery');
  h.link = find(h.root, node => node.tag === 'a');
  return h;
}
const settle = async () => { for (let i = 0; i < 20; i++) await Promise.resolve(); };
const saved = {id: 'a'.repeat(32), revision: 2, title: 'Private draft', updatedAt: 1};

test('opening a workspace or its panel leaves storage untouched; explicit autosave captures the lossless host', async () => {
  const h = fixture(); h.open(); h.editor.emit('input'); h.mutate();
  assert.equal(h.access, 0); assert.equal(h.snapshots.length, 0);
  h.click('Enable autosave'); await settle();
  assert.deepEqual(h.snapshots, [{source: '\ufeff# exact\r\n', runtime: h.payload.textContent}]);
  assert.equal(h.access, 1); assert.equal(h.writes[0].expectedRevision, 0);
  assert.ok(h.byText('Local recovery on'));
  h.source += 'edit'; h.editor.emit('input'); h.payload.textContent = 'new fonts'; h.mutate();
  assert.equal(h.timers.size, 1); h.click('Save now'); await settle();
  assert.deepEqual(h.snapshots[1], {source: h.source, runtime: 'new fonts'});
  assert.equal(h.writes[1].expectedRevision, 1);
  h.click('Turn off autosave'); h.editor.emit('input');
  assert.equal(h.timers.size, 0);
});

test('composition and suspension fence edits; returning requires a fresh explicit opt-in', async () => {
  const h = fixture(); h.open(); h.click('Enable autosave'); await settle();
  h.editor.emit('compositionstart'); h.source = 'unfinished'; h.editor.emit('input');
  assert.equal(h.timers.size, 0); assert.equal(h.byText('Save now').disabled, true);
  h.source = '完成'; h.editor.emit('compositionend');
  assert.equal(h.timers.size, 1);
  h.win.emit('pagehide'); h.win.emit('pageshow'); h.editor.emit('input');
  assert.equal(h.timers.size, 0); assert.equal(h.writes.length, 1);
  h.open(); h.click('Enable autosave'); await settle();
  assert.equal(h.snapshots[1].source, '完成');
  h.api.dispose(); assert.equal(h.disconnected, true);
});

test('listing and validating one stored revision exposes a download only inside the panel', async () => {
  const h = fixture(); h.records = [saved]; h.open(); h.click('Show saved copies'); await settle();
  const privateRow = node => node.textContent.includes('Private draft');
  assert.ok(find(h.root, privateRow)); assert.equal(find(h.container, privateRow), undefined);
  h.validation = deferred(); h.click('Prepare download'); await settle();
  assert.deepEqual(h.reads, [[saved.id, 2]]); assert.equal(h.validated, 'stored bytes');
  assert.equal(h.link.hidden, true);
  h.validation.resolve({blob: 'validated project', filename: 'draft.fmd.json'}); await settle();
  assert.equal(h.download, 'validated project'); assert.equal(h.link.download, 'draft.fmd.json');
  assert.equal(h.link.hidden, false); assert.equal(h.source, '\ufeff# exact\r\n');
  h.click('Close'); assert.equal(h.link.href, undefined); assert.deepEqual(h.revoked, ['blob:recovery']);
  assert.equal(find(h.root, privateRow), undefined); assert.equal(h.writes.length, 0);
});

test('closing or suspending retires late list and validation results', async () => {
  const h = fixture(); h.open(); h.listing = deferred(); h.click('Show saved copies'); h.click('Close');
  h.listing.resolve([saved]); await settle(); assert.equal(h.byText('Prepare download'), undefined);
  h.listing = null; h.records = [saved]; h.open(); h.click('Show saved copies'); await settle();
  h.validation = deferred(); h.click('Prepare download'); await settle(); h.win.emit('pagehide');
  assert.equal(h.cancelled, true); h.validation.resolve({blob: 'late', filename: 'late.fmd.json'}); await settle();
  assert.equal(h.download, undefined); assert.equal(h.link.hidden, true);
});

test('deletion requires confirmation and the listed revision; a conflict preserves the writer', async () => {
  const h = fixture(); h.open(); h.click('Enable autosave'); await settle();
  h.records = [saved]; h.click('Show saved copies'); await settle();
  h.click('Delete copy'); assert.equal(h.removed.length, 0);
  h.confirmed = h.conflict = true; h.click('Delete copy'); await settle();
  assert.ok(h.byText('revision changed')); assert.ok(h.byText('Local recovery on'));
  h.conflict = false; h.click('Delete copy'); await settle();
  assert.deepEqual(h.removed, [[saved.id, 2]]); assert.ok(h.byText('No local recovery copies.'));
  assert.ok(h.byText('Enable autosave')); assert.equal(h.timers.size, 0);
});

test('storage denial is reported after an explicit action and leaves the document editable', async () => {
  const h = fixture({blocked: true}); h.open(); assert.equal(h.access, 0);
  h.click('Enable autosave'); await settle();
  assert.ok(h.byText('Local recovery stopped')); assert.equal(h.writes.length, 0);
  h.click('Show saved copies'); await settle(); assert.ok(h.byText('storage denied'));
  assert.equal(h.source, '\ufeff# exact\r\n');
});
