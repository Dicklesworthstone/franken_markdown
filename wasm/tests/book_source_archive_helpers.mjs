import assert from "node:assert/strict";

// Independent ZIP reader and bitwise CRC; no production writer/checksum code.
export async function readSourceZip(blob) {
  const bytes = new Uint8Array(await blob.arrayBuffer()), view = new DataView(bytes.buffer);
  const u16 = at => view.getUint16(at, true), u32 = at => view.getUint32(at, true);
  const end = bytes.length - 22;
  assert.equal(u32(end), 0x06054b50); assert.equal(u16(end + 4), 0); assert.equal(u16(end + 6), 0);
  assert.equal(u16(end + 8), u16(end + 10)); assert.equal(u16(end + 20), 0);
  const directory = u32(end + 16), entries = new Map();
  assert.equal(directory + u32(end + 12), end);
  let at = directory, local = 0;
  for (let i = 0; i < u16(end + 10); i++) {
    assert.equal(u32(at), 0x02014b50); assert.equal(u16(at + 4), 20); assert.equal(u16(at + 6), 20);
    assert.equal(u16(at + 8), 0x800); assert.equal(u16(at + 10), 0);
    assert.equal(u16(at + 12), 0); assert.equal(u16(at + 14), 33);
    const size = u32(at + 20), nameBytes = u16(at + 28);
    assert.equal(size, u32(at + 24)); assert.equal(u16(at + 30), 0); assert.equal(u16(at + 32), 0);
    assert.equal(u16(at + 34), 0); assert.equal(u32(at + 38), 0); assert.equal(u32(at + 42), local);
    const name = new TextDecoder("utf-8", { fatal: true }).decode(bytes.subarray(at + 46, at + 46 + nameBytes));
    assert(!entries.has(name)); assert.equal(u32(local), 0x04034b50); assert.equal(u16(local + 4), 20);
    assert.equal(u16(local + 6), 0x800); assert.equal(u16(local + 8), 0); assert.equal(u16(local + 10), 0);
    assert.equal(u16(local + 12), 33); assert.equal(u32(local + 14), u32(at + 16));
    assert.equal(u32(local + 18), size); assert.equal(u32(local + 22), size);
    assert.equal(u16(local + 26), nameBytes); assert.equal(u16(local + 28), 0);
    assert.equal(new TextDecoder("utf-8", { fatal: true }).decode(bytes.subarray(local + 30, local + 30 + nameBytes)), name);
    const start = local + 30 + nameBytes;
    assert(start + size <= directory);
    const body = bytes.slice(start, start + size);
    let crc = 0xffffffff;
    for (const byte of body) {
      crc ^= byte;
      for (let bit = 0; bit < 8; bit++) crc = (crc >>> 1) ^ ((crc & 1) ? 0xedb88320 : 0);
    }
    assert.equal((crc ^ 0xffffffff) >>> 0, u32(at + 16));
    entries.set(name, body); local = start + size; at += 46 + nameBytes;
  }
  assert.equal(local, directory); assert.equal(at, end);
  return entries;
}

export class Element extends EventTarget {
  #value = "";
  constructor(tag = "div") { super(); this.tagName = tag.toUpperCase(); }
  children = []; attributes = {}; textContent = ""; hidden = false; disabled = false; checked = false;
  style = {}; selectionStart = 0; selectionEnd = 0; scrollTop = 0;
  get value() { return this.#value; }
  set value(value) { this.#value = String(value).replace(/\r\n?/g, "\n"); }
  setAttribute(key, value) { this.attributes[key] = String(value); }
  removeAttribute(key) { delete this.attributes[key]; delete this[key]; }
  appendChild(child) { child.parentElement = child.parentNode = this; this.children.push(child); return child; }
  append(...children) { for (const child of children) this.appendChild(typeof child === "string" ? Object.assign(new Element("text"), { textContent: child }) : child); }
  replaceChildren(...children) { this.children = []; this.append(...children); }
  querySelector(selector) {
    if (`#${this.id}` === selector || (selector === '[aria-labelledby="publish-title"]' && this.attributes["aria-labelledby"] === "publish-title")) return this;
    for (const node of this.children) { const found = node.querySelector(selector); if (found) return found; }
    return null;
  }
  setSelectionRange(start, end) { this.selectionStart = start; this.selectionEnd = end; }
  focus() { let root = this; while (root.parentElement) root = root.parentElement; root.activeElement = this; }
  emit(type) {
    const event = new Event(type, { cancelable: true }); Object.defineProperty(event, "target", { value: this });
    let node = this; while (node) { node.dispatchEvent(event); node = node.parentElement; }
    return event;
  }
  click() { return this.emit("click"); }
}
