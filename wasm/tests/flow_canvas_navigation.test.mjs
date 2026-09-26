// Production UI/controller protocol with explicit native-hit, session and DOM
// doubles. These tests do not claim generated-WASM or actual glyph hit testing.
import test from 'node:test';
import assert from 'node:assert/strict';
import { createCanvasNavigationControls } from '../demo/flow_canvas_navigation.mjs';

class Element extends EventTarget {
  disabled = false; value = ''; textContent = ''; scrollTop = 40; scrollLeft = 0;
  selectionStart = 0; selectionEnd = 0; focused = false;
  getBoundingClientRect() { return { left: 10, top: 20, width: 200, height: 100 }; }
  focus() { this.focused = true; this.dispatchEvent(new Event('focus')); }
  setSelectionRange(start, end) { this.selectionStart = start; this.selectionEnd = end; }
}
const deferred = () => {
  let resolve, reject;
  const promise = new Promise((yes, no) => { resolve = yes; reject = no; });
  return { promise, resolve, reject };
};
const emit = (element, type, values = {}) => {
  const event = new Event(type, { cancelable: true });
  Object.assign(event, values);
  element.dispatchEvent(event);
  return event;
};
function fixture() {
  const canvas = new Element(), viewport = new Element(), sourceEditor = new Element();
  const sourceButton = new Element(), followButton = new Element(), status = new Element();
  // Native source envelope covers "正文", NOT the fragment-local UTF-16 999.
  const source = '# 😀\r\n\r\n正文\n\n## Destination';
  sourceEditor.value = source;
  const prefix = '# 😀\r\n\r\n', end = prefix + '正文';
  const span = { startByte: Buffer.byteLength(prefix), endByte: Buffer.byteLength(end) };
  const sourceRange = { start: prefix.length, end: end.length };
  const token = { revision: '7', layoutRevision: '11' };
  const frame = { ...token, width: 100, height: 50, scrollX: 0, scrollY: 40 };
  let current = true;
  const document = {
    token, nodes: [
      { enclosingSourceSpan: span },
      { enclosingSourceSpan: { startByte: 0, endByte: 6 } },
    ],
    assertCurrent() { if (!current) throw Object.assign(new Error(), { code: 'STALE_REVISION' }); },
    locateFragment(target) {
      assert.ok(target.startsWith('#'));
      return target === '#destination' || target === '#d%C3%A9st'
        ? { ...token, nodeIndex: 1 } : null;
    },
  };
  const state = { status: 'ready', frame, document, readingPending: false, readingError: null };
  const controller = {
    state, disposed: false,
    locateReading(index, doc, input) {
      assert.equal(doc, document);
      if (input !== source || sourceEditor.value !== source)
        throw Object.assign(new Error(), { code: 'STALE_REVISION' });
      document.assertCurrent();
      return { ...token, nodeIndex: index, bounds: { x: 0, y: index ? 800 : 80, width: 100, height: 20 },
        enclosingSourceSpan: document.nodes[index].enclosingSourceSpan,
        sourceRange: index ? { start: 0, end: 4 } : sourceRange };
    },
  };
  const calls = [], navigated = [];
  let response = { schemaVersion: 1, ...token, linkTarget: '#destination', hit: {
    itemIndex: 1999, byteOffset: 999, utf16Offset: 999, clusterIndex: 999,
    coordinateSpace: 'fragment-utf16', enclosingSourceSpan: span,
  } };
  const painter = { frame, disposed: false, hitTest: async (x, y) => { calls.push([x, y]); return response; } };
  let owner = { controller, painter };
  const controls = createCanvasNavigationControls({
    canvas, viewport, sourceEditor, sourceButton, followButton, status,
    getOwner: () => owner, onNavigate: value => navigated.push(value),
  });
  const click = values => emit(canvas, 'click', { button: 0, clientX: 110, clientY: 70, ...values });
  return { canvas, viewport, sourceEditor, sourceButton, followButton, status, frame, state,
    controller, painter, controls, document, source, sourceRange, span, calls, navigated,
    click, sourceClick: () => emit(sourceButton, 'click'), followClick: () => emit(followButton, 'click'),
    setResponse(value) { response = value; }, get response() { return response; },
    replaceOwner(value) { owner = value; }, stale() { current = false; },
  };
}
async function choose(f, values) {
  f.click(values);
  await f.controls.whenIdle();
}

test('CSS-scaled clicks use logical viewport coordinates, not DPR or scroll offsets', async () => {
  const f = fixture();
  await choose(f);
  assert.deepEqual(f.calls, [[50, 25]]);
  assert.equal(f.sourceButton.disabled, false);
  assert.equal(f.followButton.disabled, false);
  assert.equal(f.sourceEditor.value, f.source);
});
test('source selection uses the exact enclosing UTF-16 mapping and keeps Markdown unchanged', async () => {
  const f = fixture();
  await choose(f);
  f.sourceClick();
  assert.equal(f.sourceEditor.focused, true);
  assert.equal(f.sourceEditor.selectionStart, f.sourceRange.start);
  assert.equal(f.sourceEditor.selectionEnd, f.sourceRange.end);
  assert.equal(f.sourceEditor.value.slice(f.sourceEditor.selectionStart, f.sourceEditor.selectionEnd), '正文');
  assert.equal(f.sourceEditor.value, f.source);
  assert.deepEqual(f.navigated, []);
});
test('only an explicit Follow action navigates to an authoritative internal heading', async () => {
  const f = fixture();
  await choose(f);
  assert.equal(f.navigated.length, 0);
  f.followClick();
  assert.equal(f.navigated.length, 1);
  assert.equal(f.navigated[0].bounds.y, 800);
  assert.equal(f.navigated[0].nodeIndex, 1);
  assert.equal(f.sourceButton.disabled, true);
  f.followClick();
  assert.equal(f.navigated.length, 1);
});
test('encoded fragment resolution is delegated intact, without a second decoder or slugger', async () => {
  const f = fixture();
  f.response.linkTarget = '#d%C3%A9st';
  await choose(f);
  f.followClick();
  assert.equal(f.navigated.length, 1);
});
test('external, unsafe, root and missing targets stay inert while source lookup still works', async () => {
  for (const target of ['https://example.com', '//example.com', 'javascript:alert(1)',
    'file:///secret', '../guide.md#section', '#missing', '#', '#a\\b', '#a\n', '#'+ 'a'.repeat(4096)]) {
    const f = fixture();
    f.response.linkTarget = target;
    await choose(f);
    assert.equal(f.followButton.disabled, true, target);
    assert.equal(f.sourceButton.disabled, false, target);
    f.followClick();
    f.sourceClick();
    assert.equal(f.navigated.length, 0);
    assert.equal(f.sourceEditor.selectionEnd, f.sourceRange.end);
  }
});
test('no-hit, foreign envelopes and malformed metadata never become guessed source locations', async () => {
  for (const hit of [null, { itemIndex: -1, enclosingSourceSpan: { startByte: 0, endByte: 6 } },
    { itemIndex: 0, enclosingSourceSpan: { startByte: 1, endByte: 3 } },
    { itemIndex: 0, enclosingSourceSpan: { startByte: 0.5, endByte: 6 } },
    { itemIndex: 0, enclosingSourceSpan: { startByte: 6, endByte: 0 } }]) {
    const f = fixture();
    f.response.hit = hit;
    await choose(f);
    assert.equal(f.sourceButton.disabled, true);
    assert.equal(f.followButton.disabled, true);
    f.sourceClick();
    assert.equal(f.sourceEditor.selectionEnd, 0);
  }
});
test('images without fragment-local text offsets support enclosing-source selection', async () => {
  const f = fixture();
  f.response.hit = { itemIndex: 5, enclosingSourceSpan: f.span };
  f.response.linkTarget = null;
  await choose(f);
  f.sourceClick();
  assert.equal(f.sourceEditor.selectionEnd, f.sourceRange.end);
});
test('edited source is rejected even before an input event or controller update', async () => {
  const f = fixture();
  await choose(f);
  f.sourceEditor.value += ' changed';
  f.sourceClick();
  assert.equal(f.sourceEditor.selectionEnd, 0);
  assert.equal(f.sourceButton.disabled, true);
  f.followClick();
  assert.equal(f.navigated.length, 0);
});
test('focus reentrancy cannot select an unrelated source after document replacement', async () => {
  const f = fixture();
  await choose(f);
  f.sourceEditor.addEventListener('focus', () => { f.sourceEditor.value = 'new document'; });
  f.sourceClick();
  assert.equal(f.sourceEditor.selectionEnd, 0);
  assert.equal(f.sourceEditor.value, 'new document');
});
test('source events, replacement with identical text and scroll revoke an issued selection', async () => {
  for (const kind of ['input', 'fmd-document-replaced', 'scroll']) {
    const f = fixture();
    await choose(f);
    emit(kind === 'scroll' ? f.viewport : f.sourceEditor, kind);
    f.sourceClick(); f.followClick();
    assert.equal(f.sourceEditor.selectionEnd, 0, kind);
    assert.equal(f.navigated.length, 0, kind);
  }
});
test('new frame with equal numeric tokens is still a different inspection owner', async () => {
  const f = fixture();
  await choose(f);
  const frame = { ...f.frame };
  f.state.frame = f.painter.frame = frame;
  f.controls.update();
  f.sourceClick();
  assert.equal(f.sourceEditor.selectionEnd, 0);
  assert.equal(f.followButton.disabled, true);
});
test('restarts with coincident tokens cannot reuse an old hit or selection', async () => {
  const f = fixture();
  const gate = deferred();
  f.painter.hitTest = () => gate.promise;
  f.click(); await Promise.resolve();
  f.replaceOwner({ controller: { ...f.controller }, painter: { ...f.painter } });
  f.controls.update();
  gate.resolve(f.response);
  await f.controls.whenIdle();
  assert.equal(f.sourceButton.disabled, true);
  assert.equal(f.navigated.length, 0);
});
test('late hits after edits are observed but never published', async () => {
  const f = fixture(), gate = deferred();
  f.painter.hitTest = () => gate.promise;
  f.click(); await Promise.resolve();
  f.sourceEditor.value += ' edit';
  emit(f.sourceEditor, 'input');
  const message = f.status.textContent;
  gate.resolve(f.response);
  await f.controls.whenIdle();
  assert.equal(f.status.textContent, message);
  assert.equal(f.sourceButton.disabled, true);
});
test('late worker rejection never replaces newer UI or becomes unhandled', async () => {
  const f = fixture(), gate = deferred();
  f.painter.hitTest = () => gate.promise;
  f.click(); await Promise.resolve();
  emit(f.sourceEditor, 'fmd-document-replaced');
  const message = f.status.textContent;
  gate.reject(Object.assign(new Error(), { code: 'SESSION_LOST' }));
  await f.controls.whenIdle();
  assert.equal(f.status.textContent, message);
  assert.equal(f.controls.busy, false);
});
test('one physical lookup remains the limit across invalidation and repeated clicks', async () => {
  const f = fixture(), gate = deferred(); let calls = 0;
  f.painter.hitTest = () => { calls++; return gate.promise; };
  f.click(); await Promise.resolve();
  for (let i = 0; i < 100; i++) f.click();
  emit(f.sourceEditor, 'input');
  f.click();
  assert.equal(calls, 1);
  gate.resolve(f.response);
  await f.controls.whenIdle();
  assert.equal(calls, 1); // No delayed retry against a different context.
  f.click(); await f.controls.whenIdle();
  assert.equal(calls, 2);
});
test('malformed or stale wire tokens cannot acquire authority', async () => {
  for (const mutation of [{ revision: '8' }, { layoutRevision: '12' }, { schemaVersion: 2 }]) {
    const f = fixture(); Object.assign(f.response, mutation);
    await choose(f);
    assert.equal(f.sourceButton.disabled, true);
    assert.match(f.status.textContent, /INVALID_HIT/);
  }
});
test('stale native document, pending reader and missing reading data refuse before native calls', async () => {
  for (const change of [f => f.stale(), f => { f.state.readingPending = true; },
    f => { f.state.document = null; }, f => { f.state.readingError = { code: 'READING_LIMIT' }; },
    f => { f.state.status = 'busy'; }, f => { f.viewport.scrollTop++; }]) {
    const f = fixture(); change(f);
    await choose(f);
    assert.equal(f.calls.length, 0);
    assert.equal(f.sourceButton.disabled, true);
  }
});
test('modified, middle, prevented and invalid-coordinate clicks do not call the worker', async () => {
  for (const values of [{ ctrlKey: true }, { altKey: true }, { metaKey: true }, { shiftKey: true },
    { button: 1 }, { clientX: NaN }, { clientY: Infinity }, { clientX: 210 }, { clientX: 9 }]) {
    const f = fixture(); await choose(f, values); assert.equal(f.calls.length, 0);
  }
  const f = fixture(), event = new Event('click', { cancelable: true });
  Object.assign(event, { button: 0, clientX: 110, clientY: 70 });
  event.preventDefault(); f.canvas.dispatchEvent(event);
  assert.equal(f.calls.length, 0);
});
test('dispose is idempotent and revokes listeners plus late completion', async () => {
  const f = fixture(), gate = deferred();
  f.painter.hitTest = () => gate.promise;
  f.click(); await Promise.resolve();
  const message = f.status.textContent;
  f.controls.dispose(); f.controls.dispose();
  gate.reject(new Error('late'));
  await f.controls.whenIdle();
  f.click(); f.sourceClick(); f.followClick();
  assert.equal(f.status.textContent, message);
  assert.equal(f.sourceButton.disabled, true);
  assert.equal(f.sourceEditor.selectionEnd, 0);
});
test('synchronous hit errors recover without a restart or a stuck busy slot', async () => {
  const f = fixture();
  f.painter.hitTest = () => { throw Object.assign(new Error(), { code: 'HIT_FAILURE' }); };
  await choose(f);
  assert.match(f.status.textContent, /HIT_FAILURE/);
  assert.equal(f.controls.busy, false);
  f.painter.hitTest = () => f.response;
  await choose(f);
  assert.equal(f.sourceButton.disabled, false);
});

test('page suspension retains physical backpressure and does not revive an old selection', async () => {
  const f = fixture(), gate = deferred(); let calls = 0;
  f.painter.hitTest = () => { calls++; return gate.promise; };
  f.click(); await Promise.resolve();
  f.controls.suspend(); f.click();
  assert.equal(f.sourceButton.disabled, true);
  f.controls.resume(); f.click();
  assert.equal(calls, 1);
  gate.resolve(f.response); await f.controls.whenIdle();
  assert.equal(f.sourceButton.disabled, true);
  f.click(); await f.controls.whenIdle();
  assert.equal(calls, 2);
  assert.equal(f.sourceButton.disabled, false);
});
