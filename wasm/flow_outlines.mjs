// Exact-glyph outline contract. Shared by the synchronous facade, worker
// normalization and Canvas backend; no font parsing or text shaping lives here.
import { FlowError, identity, normalizeFlowError } from "./flow_session.mjs";
export const MAX_OUTLINE_GLYPHS = 256;
export const MAX_OUTLINE_COMMANDS = 65536;
const fail = (code, message) => { throw new FlowError(code, message); };

export function outlineGlyphIds(value) {
  const typed = ArrayBuffer.isView(value) && Object.prototype.toString.call(value) === "[object Uint16Array]";
  if (!Array.isArray(value) && !typed) fail("INVALID_ARGUMENT", "glyphIds must be an array or Uint16Array");
  if (value.length > MAX_OUTLINE_GLYPHS) fail("BUDGET_EXCEEDED", "at most 256 glyph outlines per request");
  if (typed) {
    if (Object.prototype.toString.call(value.buffer) !== "[object ArrayBuffer]") {
      fail("INVALID_ARGUMENT", "glyphIds must not use shared memory");
    }
    try { new Uint16Array(value.buffer, value.byteOffset, value.length); }
    catch { fail("INVALID_ARGUMENT", "glyphIds buffer is detached"); }
  }
  return Array.from(value, id => {
    if (!Number.isInteger(id) || id < 0 || id > 65535) fail("INVALID_ARGUMENT", "glyph IDs must be integers in 0..=65535");
    return id;
  });
}

export function validateOutlineBatch(batch, fontId, requested) {
  const bad = () => fail("INVALID_WASM_RESPONSE", "inconsistent glyph-outline response");
  if (!batch || batch.schemaVersion !== 1 || batch.fontId !== fontId
      || !Number.isInteger(batch.unitsPerEm) || batch.unitsPerEm < 1 || batch.unitsPerEm > 65535
      || ![batch.ascent, batch.descent, batch.lineGap].every(n => Number.isInteger(n) && n >= -32768 && n <= 32767)
      || batch.ascent <= batch.descent || !Array.isArray(batch.glyphs) || batch.glyphs.length !== requested.length) bad();
  let commands = 0;
  for (let i = 0; i < requested.length; i++) {
    const glyph = batch.glyphs[i];
    if (!glyph || glyph.glyphId !== requested[i] || !Array.isArray(glyph.commands)) bad();
    commands += glyph.commands.length;
    if (commands > MAX_OUTLINE_COMMANDS) bad();
    let open = false;
    for (const command of glyph.commands) {
      if (!Array.isArray(command)) bad();
      const op = command[0];
      const size = op === "M" || op === "L" ? 3 : op === "Q" ? 5 : op === "Z" ? 1 : 0;
      if (!size || command.length !== size
          || !command.slice(1).every(n => typeof n === "number" && Number.isFinite(n) && Math.abs(n) <= 100000000)) bad();
      if (op === "M") { if (open) bad(); open = true; }
      else { if (!open) bad(); if (op === "Z") open = false; }
    }
    if (open) bad();
  }
  return batch;
}

export function withGlyphOutlines(session, read) {
  // Copy descriptors, not getter values: revision/disposal must stay live.
  const api = Object.create(Object.getPrototypeOf(session), Object.getOwnPropertyDescriptors(session));
  Object.defineProperty(api, "glyphOutlines", { enumerable: true, value(fontId, glyphIds) {
    if (session.disposed) fail("SESSION_DISPOSED", "flow session has been disposed");
    const id = identity(fontId, "fontId");
    const ids = outlineGlyphIds(glyphIds);
    try {
      const text = read(id, new Uint16Array(ids));
      if (typeof text !== "string" || text.length > 16 * 1024 * 1024) {
        fail("INVALID_WASM_RESPONSE", "glyph-outline response exceeds its JSON limit");
      }
      let value;
      try { value = JSON.parse(text); } catch { fail("INVALID_WASM_RESPONSE", "glyph-outline response is not JSON"); }
      return validateOutlineBatch(value, id, ids);
    } catch (error) { throw normalizeFlowError(error); }
  } });
  return Object.freeze(api);
}
