// Literal Markdown source edits, not an AST formatter or rendered-text editor.
// Plans use original UTF-16 coordinates; callers own history and publication.
const MAX_BYTES = 4 * 1024 * 1024;
const MAX_LINES = 10000;
export class SourceCommandError extends Error {
  constructor(code, message) { super(message); this.name = "SourceCommandError"; this.code = code; }
}
const fail = (code, message) => { throw new SourceCommandError(code, message); };
const MARKERS = { bold: "**", italic: "*", strike: "~~" };
const COMMANDS = new Set([...Object.keys(MARKERS), "inline-code", "code-block", "heading-1", "heading-2", "heading-3", "bullet-list", "ordered-list", "task-list", "blockquote"]);

// Count without allocating a second encoded document; reject malformed UTF-16.
function byteLength(text) {
  if (typeof text !== "string") fail("INVALID_SOURCE", "Markdown source must be a string.");
  let bytes = 0;
  for (let i = 0; i < text.length; i++) {
    const c = text.charCodeAt(i);
    if (c >= 0xd800 && c <= 0xdbff) {
      const low = text.charCodeAt(++i);
      if (!(low >= 0xdc00 && low <= 0xdfff)) fail("INVALID_UNICODE", "Source contains an unpaired surrogate.");
      bytes += 4;
    } else if (c >= 0xdc00 && c <= 0xdfff) fail("INVALID_UNICODE", "Source contains an unpaired surrogate.");
    else bytes += c < 128 ? 1 : c < 2048 ? 2 : 3;
    if (bytes > MAX_BYTES) fail("BUDGET_EXCEEDED", "Formatting must stay within the 4 MiB source limit.");
  }
  return bytes;
}
function boundary(source, offset) {
  const c = source.charCodeAt(offset);
  return Number.isSafeInteger(offset) && offset >= 0 && offset <= source.length
    && !(c >= 0xdc00 && c <= 0xdfff)
    && !(source[offset - 1] === "\r" && source[offset] === "\n");
}
function longestTicks(text) {
  let longest = 0, run = 0;
  for (const ch of text) { run = ch === "`" ? run + 1 : 0; longest = Math.max(longest, run); }
  return longest;
}
function lineRange(source, start, end) {
  let left = start, right = end;
  while (left > 0 && !/[\r\n]/u.test(source[left - 1])) left--;
  // A selection ending at the next line's start must not format that line.
  if (end > start && /[\r\n]/u.test(source[end - 1])) {
    right = end - (source.slice(end - 2, end) === "\r\n" ? 2 : 1);
  } else while (right < source.length && !/[\r\n]/u.test(source[right])) right++;
  return [left, right];
}
function rows(text) {
  const output = [];
  let at = 0;
  for (;;) {
    if (output.length >= MAX_LINES) fail("FORMATTING_LIMIT", "Select at most 10000 lines for one formatting command.");
    let end = at;
    while (end < text.length && !/[\r\n]/u.test(text[end])) end++;
    const eol = text[end] === "\r" && text[end + 1] === "\n" ? "\r\n" : text[end] ?? "";
    output.push({ at, text: text.slice(at, end), eol });
    if (end === text.length) return output;
    at = end + eol.length;
  }
}

/** Produce ONE bounded edit and its post-edit selection, without changing input.
 * Inline emphasis toggles literal adjacent markers. Code uses a collision-free
 * delimiter and preserves literal spaces/backticks. Line commands transform
 * complete selected lines, preserving indentation and original line endings.
 * This deliberately does not infer Markdown nesting or reformat an AST.
 */
export function planSourceCommand(source, selection, command) {
  if (!COMMANDS.has(command)) fail("UNKNOWN_COMMAND", "Unknown Markdown source command.");
  const sourceBytes = byteLength(source);
  const { start, end, direction = "none" } = selection ?? {};
  if (!boundary(source, start) || !boundary(source, end) || start > end
      || !["none", "forward", "backward"].includes(direction)) {
    fail("INVALID_SELECTION", "Select UTF-16 scalar boundaries without splitting CRLF.");
  }
  const finish = (from, to, text, a, b) => {
    if (sourceBytes - byteLength(source.slice(from, to)) + byteLength(text) > MAX_BYTES)
      fail("BUDGET_EXCEEDED", "Formatting would exceed the 4 MiB source limit.");
    return Object.freeze({ start: from, end: to, text,
      selection: Object.freeze({ start: a, end: b, direction }) });
  };
  const selected = source.slice(start, end);
  if (Object.hasOwn(MARKERS, command) || command === "inline-code") {
    if (/[\r\n]/u.test(selected)) fail("MULTILINE_SELECTION", "Use a code block or select one line for inline formatting.");
    if (command === "inline-code") {
      const content = selected || "code";
      const fence = "`".repeat(longestTicks(content) + 1);
      const pad = content.startsWith("`") || content.endsWith("`")
        || (content.startsWith(" ") && content.endsWith(" ") && /[^ ]/u.test(content)) ? " " : "";
      const prefix = fence + pad;
      return finish(start, end, prefix + content + pad + fence, start + prefix.length, start + prefix.length + content.length);
    }
    const marker = MARKERS[command], n = marker.length;
    // A two-star run is strong, not removable one-star emphasis. Three stars
    // can carry both. Refuse ambiguous/asymmetric runs instead of stripping
    // half of another command's delimiter when applying nested formatting.
    const runAt = (text, at, step) => {
      let count = 0;
      while (text[at] === marker[0]) { count++; at += step; }
      return count;
    };
    const removable = (left, right) => left === right && (command === "strike"
      ? left === 2 : left >= n && left <= 3 && (n === 2 || left % 2 === 1));
    if (selected.length > 2 * n && removable(runAt(selected, 0, 1), runAt(selected, selected.length - 1, -1))) {
      const content = selected.slice(n, -n);
      return finish(start, end, content, start, start + content.length);
    }
    if (removable(runAt(source, start - 1, -1), runAt(source, end, 1))) {
      return finish(start - n, end + n, selected, start - n, end - n);
    }
    const leading = selected.match(/^[ \t]*/u)[0];
    const rest = selected.slice(leading.length), trailing = rest.match(/[ \t]*$/u)[0];
    const content = rest.slice(0, rest.length - trailing.length) || "text";
    const prefix = leading + marker;
    return finish(start, end, prefix + content + marker + trailing, start + prefix.length, start + prefix.length + content.length);
  }
  const [from, to] = lineRange(source, start, end), body = source.slice(from, to);
  const lines = rows(body);
  if (command === "code-block") {
    const eol = lines.find(line => line.eol)?.eol
      || (source.slice(to, to + 2) === "\r\n" ? "\r\n" : source[to] === "\r" ? "\r" : "\n");
    const fence = "`".repeat(Math.max(3, longestTicks(body) + 1));
    const prefix = fence + eol;
    return finish(from, to, prefix + body + eol + fence, from + prefix.length, from + prefix.length + body.length);
  }
  const heading = command.startsWith("heading-") ? Number(command.at(-1)) : 0;
  const classify = text => {
    const indent = text.match(/^[ \t]*/u)[0], content = text.slice(indent.length);
    let existing = "", matches = false;
    if (heading) {
      existing = content.match(/^#{1,6}(?:[ \t]+|$)/u)?.[0] ?? "";
      matches = existing.trim().length === heading;
    } else if (command === "blockquote") {
      existing = content.match(/^>(?:[ \t]?)/u)?.[0] ?? "";
      matches = existing !== "";
    } else {
      const task = content.match(/^[-+*][ \t]+\[[ xX]\](?:[ \t]+|$)/u)?.[0];
      const bullet = content.match(/^[-+*](?:[ \t]+|$)/u)?.[0];
      const ordered = content.match(/^[0-9]{1,9}[.)](?:[ \t]+|$)/u)?.[0];
      existing = task ?? bullet ?? ordered ?? "";
      matches = command === "task-list" ? !!task : command === "ordered-list" ? !!ordered : !!bullet && !task;
    }
    return { indent, content, existing, matches };
  };
  const parsed = lines.map(line => ({ ...line, ...classify(line.text) }));
  const applies = line => line.content.length > 0 || (start === end && lines.length === 1);
  const active = parsed.filter(applies);
  const remove = active.length > 0 && active.every(line => line.matches);
  let ordinal = 0, delta = 0, caret = start;
  const fragments = parsed.map(line => {
    if (!applies(line)) return line.text + line.eol;
    ordinal++;
    const prefix = remove ? "" : heading ? "#".repeat(heading) + " "
      : command === "blockquote" ? "> " : command === "ordered-list" ? `${ordinal}. `
      : command === "task-list" ? "- [ ] " : "- ";
    const at = from + line.at + line.indent.length;
    const removed = line.existing.length;
    if (start >= at) {
      caret = start < at + removed ? at + delta + prefix.length : start + delta + prefix.length - removed;
    }
    delta += prefix.length - removed;
    return line.indent + prefix + line.content.slice(removed) + line.eol;
  });
  const text = fragments.join("");
  return finish(from, to, text, start === end ? caret : from, start === end ? caret : from + text.length);
}
