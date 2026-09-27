// File-I/O admission tests for the production authoring asset. No real fonts.
import assert from 'node:assert/strict';
import {test} from 'node:test';
import {readFileSync} from 'node:fs';
import vm from 'node:vm';
const code = readFileSync(new URL('../src/interactive_fonts.js', import.meta.url), 'utf8');
const context = vm.createContext({}); vm.runInContext(code, context);
const api = vm.runInContext('FmdFontFiles', context);
const file = (name = 'face.ttf', size = 3) => ({name, size});
function host() {
  const readers = [], timers = new Map(); let timerId = 0;
  class FileReader {
    constructor() { readers.push(this); this.readyState = 0; }
    readAsArrayBuffer(file) { this.file = file; this.readyState = 1; }
    abort() { this.readyState = 2; this.aborted = true; this.onabort?.(); }
    finish(bytes) { this.result = bytes.buffer; this.readyState = 2; this.onload?.(); }
  }
  return {FileReader, readers, timers,
    setTimeout(fn, delay) { timers.set(++timerId, {fn, delay}); return timerId; },
    clearTimeout(id) { timers.delete(id); }};
}

test('local TTF metadata is captured with exact case-insensitive extension and bounded length', () => {
  const selected = file('字體.TTF'), item = api.capture(selected);
  assert.equal(item.file, selected); assert.equal(item.size, 3); assert.ok(Object.isFrozen(item));
  assert.equal(api.capture(file('f.ttf', api.limits.fileBytes)).size, api.limits.fileBytes);
});

test('unsupported names, controls, sizes and empty fonts reject before reading', () => {
  for (const name of ['face.otf', 'face.woff2', 'face.ttc', 'face.ttf.exe', '', 'a\0.ttf', 'a\n.ttf', 'x'.repeat(1024)+'.ttf', 1, null]) {
    assert.throws(() => api.capture(file(name)), /local .ttf/);
  }
  for (const size of [0, -1, 1.2, Infinity, NaN, '3', api.limits.fileBytes + 1]) assert.throws(() => api.capture(file('f.ttf', size)), /32 MiB/);
  assert.throws(() => api.capture(null));
});

test('file reading owns the admitted exact bytes and clears handlers/timer/cancellation', async () => {
  const win = host(), op = {}, selected = file(); let checks = 0;
  const promise = api.read(api.capture(selected), win, op, () => checks++);
  assert.equal(win.readers[0].file, selected); assert.equal(win.timers.size, 1);
  win.readers[0].finish(new Uint8Array([1, 2, 3]));
  assert.deepEqual(Array.from(await promise), [1, 2, 3]); assert.equal(checks, 2);
  assert.equal(win.timers.size, 0); assert.equal(op.cancelRead, null);
  assert.equal(win.readers[0].onload, null);
});

test('a rejected snapshot never starts a file read', async () => {
  const win = host(), op = {};
  await assert.rejects(api.read(api.capture(file()), win, op, () => { throw Error('stale'); }), /stale/);
  assert.equal(win.readers[0].readyState, 0); assert.equal(win.timers.size, 0);
});

test('a stale completion discards bytes without passing them to the renderer', async () => {
  const win = host(), op = {}; let current = true;
  const promise = api.read(api.capture(file()), win, op, () => { if (!current) throw Error('changed'); });
  current = false; win.readers[0].finish(new Uint8Array([1, 2, 3]));
  await assert.rejects(promise, /changed/); assert.equal(op.cancelRead, null);
});

test('cancellation aborts the read and late completion cannot resolve it', async () => {
  const win = host(), op = {};
  const promise = api.read(api.capture(file()), win, op, () => {});
  const callback = win.readers[0].onload;
  op.cancelRead(); assert.ok(win.readers[0].aborted);
  win.readers[0].finish(new Uint8Array([1, 2, 3])); callback();
  await assert.rejects(promise, /cancelled/);
  assert.equal(win.timers.size, 0); assert.equal(op.cancelRead, null);
});

test('size mismatch and non-ArrayBuffer results fail closed', async () => {
  for (const result of [new Uint8Array([1]).buffer, null, new SharedArrayBuffer(3)]) {
    const win = host(), op = {};
    const promise = api.read(api.capture(file()), win, op, () => {});
    win.readers[0].result = result; win.readers[0].readyState = 2; win.readers[0].onload();
    await assert.rejects(promise); assert.equal(win.timers.size, 0);
  }
});

test('timeout retires handlers and aborts the pending local read', async () => {
  const win = host(), op = {};
  const promise = api.read(api.capture(file()), win, op, () => {});
  const timer = [...win.timers.values()][0]; assert.equal(timer.delay, 10000); timer.fn();
  await assert.rejects(promise, /ten seconds/);
  assert.ok(win.readers[0].aborted); assert.equal(op.cancelRead, null);
});

test('read errors, abort events and synchronous Reader failures release resources', async () => {
  for (const kind of ['error', 'abort', 'throw']) {
    const win = host(), op = {};
    if (kind === 'throw') win.FileReader.prototype.readAsArrayBuffer = () => { throw Error('denied'); };
    const promise = api.read(api.capture(file()), win, op, () => {});
    if (kind !== 'throw') win.readers[0]['on' + kind]();
    await assert.rejects(promise); assert.equal(win.timers.size, 0); assert.equal(op.cancelRead, null);
  }
});
