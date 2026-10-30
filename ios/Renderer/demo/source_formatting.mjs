// Source-only authoring UI. No renderer, worker, storage or secondary undo stack.
import { planSourceCommand, SourceCommandError } from "./source_commands.mjs";
const LABELS = [
  ["bold", "Bold"], ["italic", "Italic"], ["strike", "Strike"],
  ["inline-code", "Inline code"], ["code-block", "Code block"],
  ["heading-1", "Heading 1"], ["heading-2", "Heading 2"], ["heading-3", "Heading 3"],
  ["bullet-list", "Bullets"], ["ordered-list", "Numbered list"],
  ["task-list", "Task list"], ["blockquote", "Quote"],
];
const fail = (code, message) => { throw new SourceCommandError(code, message); };

/** Install ordinary keyboard-accessible buttons next to the source editor.
 * One accepted command emits beforeinput + input, so the existing source undo,
 * preview, dirty-state tracking, draft scheduling and downloads see one edit.
 * beforeinput may veto the edit. Recheck source, selection and lifecycle after
 * focus and event handlers; never overwrite a replacement document.
 */
export function createSourceFormattingControls({ sourceEditor, status, container = sourceEditor.parentNode }) {
  const doc = sourceEditor.ownerDocument;
  const group = doc.createElement("div");
  group.setAttribute("role", "group");
  group.setAttribute("aria-label", "Markdown source formatting");
  group.setAttribute("data-source-formatting", "");
  const help = doc.createElement("p");
  help.textContent = "Format selected source: Ctrl/Cmd+B bold, Ctrl/Cmd+I italic, Ctrl/Cmd+Shift+X strike. Line commands affect complete lines. Code commands insert literal delimiters; Undo restores the edit.";
  group.append(help);
  let disposed = false, composing = false, busy = false, epoch = 0;
  const listeners = [], buttons = [];
  const listen = (target, name, handler) => {
    target.addEventListener(name, handler);
    listeners.push(() => target.removeEventListener(name, handler));
  };
  const writable = () => !disposed && !composing && !sourceEditor.readOnly && !sourceEditor.disabled;
  const refresh = () => { for (const button of buttons) button.disabled = busy || !writable(); };
  const capture = () => ({ source: sourceEditor.value, start: sourceEditor.selectionStart,
    end: sourceEditor.selectionEnd, direction: sourceEditor.selectionDirection ?? "none", epoch });
  const same = before => {
    const now = capture();
    return Object.keys(before).every(key => now[key] === before[key]);
  };
  const event = (type, cancelable, text) => {
    const ctor = doc.defaultView?.InputEvent;
    return ctor ? new ctor(type, { bubbles: true, cancelable, inputType: "insertReplacementText", data: text })
      : new (doc.defaultView?.Event ?? Event)(type, { bubbles: true, cancelable });
  };
  function execute(command) {
    if (disposed) fail("SESSION_DISPOSED", "Source formatting controls are disposed.");
    if (!writable() || busy) fail("EDITOR_BUSY", "Finish composing or unlock the source editor before formatting.");
    const before = capture();
    const plan = planSourceCommand(before.source, before, command);
    const expected = before.source.slice(0, plan.start) + plan.text + before.source.slice(plan.end);
    if (expected === before.source) return false;
    busy = true;
    refresh();
    const fence = () => {
      if (!writable() || !same(before)) fail("STALE_SOURCE", "Source or selection changed before formatting; no command was applied.");
    };
    try {
      sourceEditor.focus({ preventScroll: true });
      fence();
      const accepted = sourceEditor.dispatchEvent(event("beforeinput", true, plan.text));
      fence();
      if (!accepted) return false;
      // All admission and cancelable host callbacks precede the first mutation.
      // Do not emulate a rollback after input listeners have observed the edit.
      sourceEditor.setRangeText(plan.text, plan.start, plan.end, "preserve");
      try {
        sourceEditor.setSelectionRange(plan.selection.start, plan.selection.end, plan.selection.direction);
      } finally {
        sourceEditor.dispatchEvent(event("input", false, plan.text));
      }
      if (!disposed && sourceEditor.value === expected)
        status.textContent = "Applied Markdown source formatting. Undo restores this edit; no rendered text was copied back into source.";
      return true;
    } finally { busy = false; refresh(); }
  }
  const run = command => {
    if (disposed) return;
    try { execute(command); }
    catch (error) {
      if (!disposed) status.textContent = `${error?.code ?? "FORMATTING_ERROR"}: ${error?.message ?? "Source command failed."}`;
    }
  };
  for (const [command, label] of LABELS) {
    const button = doc.createElement("button");
    button.type = "button";
    button.textContent = label;
    button.setAttribute("data-source-command", command);
    if (sourceEditor.id) button.setAttribute("aria-controls", sourceEditor.id);
    button.style.margin = "0 6px 6px 0";
    button.style.minHeight = "44px";
    listen(button, "click", e => { if (!e.defaultPrevented) run(command); });
    buttons.push(button);
    group.append(button);
  }
  listen(sourceEditor, "input", () => { epoch++; refresh(); });
  listen(sourceEditor, "fmd-document-replaced", () => { epoch++; composing = false; refresh(); });
  listen(sourceEditor, "compositionstart", () => { epoch++; composing = true; refresh(); });
  listen(sourceEditor, "compositionend", () => { epoch++; composing = false; refresh(); });
  listen(sourceEditor, "focus", refresh);
  listen(sourceEditor, "keydown", e => {
    if (e.defaultPrevented || e.repeat || e.isComposing || e.keyCode === 229 || e.altKey
        || !(e.ctrlKey || e.metaKey) || !writable() || busy) return;
    const key = e.key?.toLowerCase();
    const command = !e.shiftKey && key === "b" ? "bold" : !e.shiftKey && key === "i" ? "italic"
      : e.shiftKey && key === "x" ? "strike" : null;
    if (command) { e.preventDefault(); run(command); }
  });
  const Observer = doc.defaultView?.MutationObserver;
  const observer = Observer ? new Observer(refresh) : null;
  observer?.observe(sourceEditor, { attributes: true, attributeFilter: ["readonly", "disabled"] });
  if (container === sourceEditor.parentNode) container.insertBefore(group, sourceEditor.nextSibling);
  else container.append(group);
  refresh();
  return Object.freeze({
    execute,
    dispose() {
      if (disposed) return;
      disposed = true;
      epoch++;
      observer?.disconnect();
      for (const stop of listeners) stop();
      listeners.length = 0;
      refresh();
      group.remove();
    },
  });
}
