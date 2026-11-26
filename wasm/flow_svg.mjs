// Static SVG admission, not a renderer. Markers and patterns are excluded:
// their per-vertex/tile multiplication is not bounded by a reference DAG alone. The browser decodes a bounded,
// canonical preview; callers retain the original authorized bytes for export.
const NS = "http://www.w3.org/2000/svg";
const XLINK = "http://www.w3.org/1999/xlink";
const MAX_BYTES = 2 * 1024 * 1024;
const MAX_NODES = 8192;
const MAX_DEPTH = 64;
const MAX_REFERENCES = 4096;
const MAX_EXPANDED_NODES = 65536;
const MAX_EXPANDED_BYTES = 8 * 1024 * 1024;
const elements = new Set(("svg g defs symbol use path rect circle ellipse line polyline polygon " +
  "text tspan textPath linearGradient radialGradient stop clipPath mask title desc").split(" "));
const presentation = new Set(("fill fill-opacity fill-rule stroke stroke-width stroke-opacity " +
  "stroke-linecap stroke-linejoin stroke-miterlimit stroke-dasharray stroke-dashoffset opacity color " +
  "clip-path clip-rule mask font-family font-size font-style " +
  "font-weight text-anchor dominant-baseline alignment-baseline letter-spacing word-spacing " +
  "text-decoration paint-order vector-effect display visibility shape-rendering text-rendering " +
  "stop-color stop-opacity").split(" "));
const attributes = new Set(("id class x y x1 y1 x2 y2 cx cy r rx ry dx dy d points width height " +
  "viewBox preserveAspectRatio transform gradientTransform gradientUnits spreadMethod offset " +
  "fx fy fr clipPathUnits maskUnits maskContentUnits " +
  "pathLength textLength lengthAdjust startOffset method spacing rotate " +
  "version baseProfile").split(" "));
const referenceProperties = new Set(["fill", "stroke", "clip-path", "mask"]);
const hrefElements = new Set(["use", "textPath", "linearGradient", "radialGradient"]);
const colourFunctions = new Set(["rgb", "rgba", "hsl", "hsla"]);
const transformFunctions = new Set(["matrix", "translate", "scale", "rotate", "skewX", "skewY"]);
const identifier = /^[A-Za-z_][A-Za-z0-9_.:-]{0,127}$/;
const number = "[+-]?(?:[0-9]+(?:\\.[0-9]*)?|\\.[0-9]+)(?:[eE][+-]?[0-9]+)?";
const dimension = new RegExp("^(" + number + ")(px|pt|pc|mm|cm|in|%)?$");
const separator = "(?:[ \\t\\r\\n]+(?:,[ \\t\\r\\n]*)?|,[ \\t\\r\\n]*)";
const viewBox = new RegExp("^[ \\t\\r\\n]*(" + number + ")" + separator + "(" + number +
  ")" + separator + "(" + number + ")" + separator + "(" + number + ")[ \\t\\r\\n]*$");
const escapeText = value => value.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;");
const escapeAttribute = value => escapeText(value).replace(/"/g, "&quot;");
const utf8Length = value => {
  let bytes = 0;
  for (const ch of value) {
    const scalar = ch.codePointAt(0);
    bytes += scalar < 0x80 ? 1 : scalar < 0x800 ? 2 : scalar < 0x10000 ? 3 : 4;
  }
  return bytes;
};

/** Pure admission seam. Input has already passed strict UTF-8 byte decoding.
 * Supported root sizes are positive CSS lengths, or an explicit viewBox.
 * Missing/percentage dimensions use that box and preserve its aspect ratio.
 * Raster dimensions round upward and are written only into the preview. */
export function parseStaticSvg(source, failure) {
  const reject = (message, code = "UNSUPPORTED_IMAGE") => { throw failure(code, message); };
  const invalid = () => reject("invalid static SVG structure", "INVALID_IMAGE");
  if (typeof source !== "string") invalid();
  if (source.length > MAX_BYTES || utf8Length(source) > MAX_BYTES)
    reject("SVG exceeds the 2 MiB source limit", "ASSET_BYTE_LIMIT");
  const validScalar = value => value === 9 || value === 10 || value === 13 ||
    value >= 32 && value <= 0x10ffff && !(value >= 0xd800 && value <= 0xdfff) &&
    value !== 0xfffe && value !== 0xffff;
  for (const ch of source) if (!validScalar(ch.codePointAt(0))) invalid();
  const entities = value => {
    let result = "", copied = 0, start = value.indexOf("&");
    while (start >= 0) {
      // XML has only five named entities in this profile. Numeric scalars need
      // at most eight significant characters; bound extra leading zeros too.
      // Inspect a fixed window so unterminated ampersands cannot cause rescans.
      const tail = value.slice(start + 1, start + 18), close = tail.indexOf(";");
      if (close < 1) invalid();
      const body = tail.slice(0, close);
      const known = { amp: "&", lt: "<", gt: ">", quot: '"', apos: "'" };
      let decoded;
      if (Object.hasOwn(known, body)) decoded = known[body];
      else {
        if (!/^#(?:[0-9]+|x[0-9a-fA-F]+)$/.test(body)) invalid();
        const scalar = body[1] === "x" ? parseInt(body.slice(2), 16) : Number(body.slice(1));
        if (!validScalar(scalar)) invalid();
        decoded = String.fromCodePoint(scalar);
      }
      result += value.slice(copied, start) + decoded;
      copied = start + close + 2;
      start = value.indexOf("&", copied);
    }
    return result + value.slice(copied);
  };
  let first = source.charCodeAt(0) === 0xfeff ? 1 : 0, last = source.length;
  while (first < last && " \t\r\n".includes(source[first])) first++;
  while (last > first && " \t\r\n".includes(source[last - 1])) last--;
  source = source.slice(first, last);
  if (source.startsWith("<?xml")) {
    const declaration = /^<\?xml[ \t\r\n]+version[ \t\r\n]*=[ \t\r\n]*(["'])1\.0\1(?:[ \t\r\n]+encoding[ \t\r\n]*=[ \t\r\n]*(["'])[Uu][Tt][Ff]-8\2)?(?:[ \t\r\n]+standalone[ \t\r\n]*=[ \t\r\n]*(["'])(?:yes|no)\3)?[ \t\r\n]*\?>/.exec(source);
    if (!declaration) invalid();
    source = source.slice(declaration[0].length);
  }
  const stack = [], ids = new Map(), references = [];
  let root = null, offset = 0, nodeCount = 0, xlink = false;
  const charge = () => { if (++nodeCount > MAX_NODES) reject("SVG exceeds the 8192-node limit"); };
  const addReference = (node, value) => {
    const id = value.startsWith("#") ? value.slice(1) : "";
    if (!identifier.test(id)) reject("SVG references must be simple local fragment identifiers");
    if (references.length === MAX_REFERENCES) reject("SVG exceeds the 4096-reference limit");
    references.push({ node, id });
  };
  const paint = (node, key, value) => {
    // No CSS escape/comment interpretation, custom properties, imports,
    // downloaded fonts, calc/var indirection or image-set resource functions.
    if (/[\\@]/.test(value) || value.includes("/*") || value.includes("*/"))
      reject("SVG CSS escapes, comments and at-rules are not supported");
    if (/url[ \t\r\n]*\(/i.test(value)) {
      if (!referenceProperties.has(key)) reject("SVG resource references are not supported here");
      const local = /^url\([ \t\r\n]*(["']?)(#[A-Za-z_][A-Za-z0-9_.:-]{0,127})\1[ \t\r\n]*\)$/.exec(value);
      if (!local) reject("SVG paint resources must be local fragment references");
      addReference(node, local[2]);
      return;
    }
    // Match each identifier once, then inspect its suffix. A regex requiring
    // an opening parenthesis would retry every suffix of a long bare word.
    for (const match of value.matchAll(/[A-Za-z_-][A-Za-z0-9_-]*/g)) {
      let end = match.index + match[0].length;
      while (end < value.length && " \t\r\n".includes(value[end])) end++;
      if (value[end] !== "(") continue;
      const allowed = key === "transform" || key === "gradientTransform" ? transformFunctions : colourFunctions;
      if (!allowed.has(match[0])) reject("unsupported SVG value function");
    }
  };
  const attribute = (node, key, value) => {
    if (key === "xmlns" || key === "xmlns:xlink") {
      if (node !== root || value !== (key === "xmlns" ? NS : XLINK))
        reject("SVG namespace changes are not supported");
      if (key === "xmlns:xlink") xlink = true;
      return;
    }
    if (key === "href" || key === "xlink:href") {
      if (!hrefElements.has(node.name) || node.attributes.has("href") || key === "xlink:href" && !xlink)
        reject("unsupported, unbound or ambiguous SVG href");
      addReference(node, value);
      node.attributes.set("href", value);
      return;
    }
    if (key === "style") {
      const styles = [];
      for (const declaration of value.split(";")) {
        if (!declaration.trim()) continue;
        const colon = declaration.indexOf(":");
        const property = declaration.slice(0, colon).trim();
        const content = declaration.slice(colon + 1).trim();
        if (colon < 1 || !presentation.has(property) || !content)
          reject("SVG inline styles support only static presentation properties");
        paint(node, property, content);
        styles.push(property + ":" + content);
      }
      node.attributes.set(key, styles.join(";"));
      return;
    }
    if (!attributes.has(key) && !presentation.has(key)) reject("unsupported SVG attribute: " + key);
    if (key === "id") {
      if (!identifier.test(value) || ids.has(value)) reject("SVG IDs must be unique simple identifiers");
      ids.set(value, node);
    }
    paint(node, key, value);
    node.attributes.set(key, value);
  };
  const addText = (value, literal = false) => {
    if (!stack.length) { if (!/^[ \t\r\n]*$/.test(value)) invalid(); return; }
    if (value.includes("]]>")) invalid();
    charge();
    stack.at(-1).children.push(literal ? value : entities(value));
  };
  while (offset < source.length) {
    if (source[offset] !== "<") {
      const end = source.indexOf("<", offset);
      const next = end < 0 ? source.length : end;
      addText(source.slice(offset, next));
      offset = next;
      continue;
    }
    if (source.startsWith("<!--", offset)) {
      const end = source.indexOf("-->", offset + 4);
      if (end < 0 || source.slice(offset + 4, end).includes("--") || end > offset + 4 && source[end - 1] === "-") invalid();
      offset = end + 3;
      continue;
    }
    if (source.startsWith("<![CDATA[", offset)) {
      const end = source.indexOf("]]>", offset + 9);
      if (end < 0 || !stack.length) invalid();
      addText(source.slice(offset + 9, end), true);
      offset = end + 3;
      continue;
    }
    if (source.startsWith("<!", offset) || source.startsWith("<?", offset))
      reject("SVG declarations, processing instructions and entities are not supported");
    if (source.startsWith("</", offset)) {
      const close = /^<\/([A-Za-z][A-Za-z0-9]*)[ \t\r\n]*>/.exec(source.slice(offset));
      if (!close || stack.pop()?.name !== close[1]) invalid();
      offset += close[0].length;
      continue;
    }
    const open = /^<([A-Za-z][A-Za-z0-9]*)(?=[ \t\r\n/>])/.exec(source.slice(offset));
    if (!open) invalid();
    if (!elements.has(open[1])) reject("unsupported SVG element: " + open[1]);
    const node = { name: open[1], attributes: new Map(), children: [], edges: [] };
    charge();
    if (!stack.length) {
      if (root || node.name !== "svg") invalid();
      root = node;
    } else stack.at(-1).children.push(node);
    if (stack.length >= MAX_DEPTH) reject("SVG exceeds the 64-level nesting limit");
    offset += open[0].length;
    const seen = new Set();
    let closed = false;
    while (offset < source.length) {
      const spaces = /^[ \t\r\n]*/.exec(source.slice(offset))[0].length;
      offset += spaces;
      if (source.startsWith("/>", offset)) { offset += 2; closed = true; break; }
      if (source[offset] === ">") { offset++; break; }
      if (!spaces) invalid();
      const entry = /^([A-Za-z_][A-Za-z0-9_.:-]*)[ \t\r\n]*=[ \t\r\n]*(["'])/.exec(source.slice(offset));
      if (!entry || seen.has(entry[1])) invalid();
      seen.add(entry[1]);
      offset += entry[0].length;
      const end = source.indexOf(entry[2], offset);
      if (end < 0 || source.slice(offset, end).includes("<")) invalid();
      const value = entities(source.slice(offset, end).replace(/\r\n|[\r\n\t]/g, " "));
      attribute(node, entry[1], value);
      offset = end + 1;
    }
    if (!closed) stack.push(node);
  }
  if (!root || stack.length) invalid();
  for (const { node, id } of references) {
    const target = ids.get(id);
    if (!target) reject("SVG references must resolve inside the selected image");
    node.edges.push(target);
  }
  // Bound the expanded reference graph, including repeated references to an
  // already visited subtree. Depth metadata prevents memoization bypasses.
  const weights = new Map(), visiting = new Set();
  const weight = (node, depth) => {
    if (visiting.has(node) || depth > MAX_DEPTH) reject("cyclic or deeply expanded SVG references");
    const previous = weights.get(node);
    if (previous) {
      if (depth + previous.height - 1 > MAX_DEPTH) reject("deeply expanded SVG references");
      return previous;
    }
    visiting.add(node);
    let size = 1, height = 1, bytes = node.name.length + 5;
    for (const [key, value] of node.attributes) bytes += key.length + utf8Length(value) + 4;
    for (const child of node.children) if (typeof child === "string") bytes += utf8Length(child);
    for (const child of [...node.children.filter(value => typeof value !== "string"), ...node.edges]) {
      const result = weight(child, depth + 1);
      size += result.size;
      bytes += result.bytes;
      height = Math.max(height, result.height + 1);
      if (size > MAX_EXPANDED_NODES) reject("SVG exceeds the 65536-node expanded scene limit");
      if (bytes > MAX_EXPANDED_BYTES) reject("SVG exceeds the 8 MiB expanded content limit");
    }
    visiting.delete(node);
    const result = { size, height, bytes };
    weights.set(node, result);
    return result;
  };
  weight(root, 1);
  const rawBox = root.attributes.get("viewBox");
  const matchBox = rawBox === undefined ? null : viewBox.exec(rawBox);
  if (rawBox !== undefined && !matchBox) invalid();
  const box = matchBox?.slice(1).map(Number);
  if (box && (!box.every(Number.isFinite) || box[2] <= 0 || box[3] <= 0)) invalid();
  const length = name => {
    const value = root.attributes.get(name);
    if (value === undefined) return null;
    const match = dimension.exec(value.trim());
    if (!match || !Number.isFinite(Number(match[1])) || Number(match[1]) <= 0) invalid();
    if (match[2] === "%") { if (!box) invalid(); return null; }
    return Number(match[1]) * ({ px: 1, pt: 4 / 3, pc: 16, mm: 96 / 25.4, cm: 96 / 2.54, in: 96 }[match[2]] ?? 1);
  };
  let width = length("width"), height = length("height");
  if (width === null) width = box ? height === null ? box[2] : height * box[2] / box[3] : null;
  if (height === null) height = box ? width * box[3] / box[2] : null;
  if (width === null || height === null) reject("SVG needs positive dimensions or a viewBox");
  width = Math.ceil(width);
  height = Math.ceil(height);
  if (!Number.isSafeInteger(width) || !Number.isSafeInteger(height) || width < 1 || height < 1)
    reject("SVG has invalid raster dimensions", "ASSET_PIXEL_LIMIT");
  root.attributes.set("xmlns", NS);
  root.attributes.set("width", String(width));
  root.attributes.set("height", String(height));
  const serialize = node => typeof node === "string" ? escapeText(node) :
    "<" + node.name + [...node.attributes].map(([key, value]) => " " + key + '="' + escapeAttribute(value) + '"').join("") +
    ">" + node.children.map(serialize).join("") + "</" + node.name + ">";
  const preview = serialize(root), previewByteLength = utf8Length(preview);
  if (previewByteLength > MAX_BYTES) reject("SVG preview exceeds the 2 MiB limit", "ASSET_BYTE_LIMIT");
  return { mime: "image/svg+xml", width, height, preview, previewByteLength };
}
