// Reuse source commands without owning a worker, save, or second undo journal.
import { createSourceFormattingControls } from "./source_formatting.mjs";

export function createBookSourceFormattingControls({ root, controls, collection }) {
  const sourceEditor = root.querySelector("#chapter-source"), status = root.querySelector("#status");
  const pathEditor = root.querySelector("#chapter-path");
  let formatter = null, disposed = false, composingPath = false;
  const refresh = () => formatter?.refresh();
  const startComposition = () => { composingPath = true; refresh(); };
  const endComposition = () => { composingPath = false; refresh(); };
  const suspend = () => { formatter?.dispose(); formatter = null; composingPath = false; };
  const resume = () => {
    if (disposed) return;
    suspend();
    formatter = createSourceFormattingControls({
      sourceEditor, status,
      isBusy: () => composingPath || controls.sourceBusy || collection.files.length === 0,
      // Source/selection alone cannot distinguish identical chapters or a
      // byte-identical project replacement during focus/beforeinput callbacks.
      checkpoint: () => JSON.stringify([collection.sourceProjectRevision, controls.checkpoint()]),
    });
  };
  resume();
  pathEditor.addEventListener("compositionstart", startComposition);
  pathEditor.addEventListener("compositionend", endComposition);
  const unsubscribe = collection.subscribe(refresh), unsubscribeBusy = controls.subscribeSourceState(refresh);
  return Object.freeze({
    execute(command) {
      if (!formatter) throw Object.assign(new Error("Book source formatting is not active."), { code: "SESSION_DISPOSED" });
      return formatter.execute(command);
    },
    suspend, resume,
    dispose() {
      if (disposed) return;
      disposed = true;
      pathEditor.removeEventListener("compositionstart", startComposition);
      pathEditor.removeEventListener("compositionend", endComposition);
      unsubscribe(); unsubscribeBusy(); suspend();
    },
  });
}
