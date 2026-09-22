import type { FlowReadingInlineRun, FlowReadingNode } from "./flow.js";
import type {
  FlowReadingInlineEntry,
  FlowReadingLinkActivation,
  ReadingFlowSession,
} from "./flow-reader.js";
import { FlowReaderView, readFlowDocument } from "./flow-reader.js";

declare const element: HTMLElement;
declare const session: ReadingFlowSession;
const reader = new FlowReaderView(element, {
  onLink(activation) {
    const typed: FlowReadingLinkActivation = activation;
    const target: string = typed.target;
    const sourceByte: number = typed.location.enclosingSourceSpan.startByte;
    const start: number | null = typed.startUtf16;
    void [target, sourceByte, start];
    // @ts-expect-error callbacks cannot mutate admitted link identity
    typed.link.target = "changed";
  },
});
const snapshot = await readFlowDocument(session, {
  limits: { maxInlineRuns: 200, maxLinkUnits: 2000 },
});
reader.render(snapshot);
const entry: FlowReadingInlineEntry | undefined = snapshot.nodes[0]?.inlineRuns[0];
if (entry) {
  const raw: FlowReadingInlineRun = entry;
  const start: number = entry.startUtf16;
  void [raw, start];
  // @ts-expect-error reading styles are immutable
  entry.style.bold = false;
}
const legacy: FlowReadingNode = {
  role: "paragraph",
  text: "plain",
  children: [],
  bounds: { x: 0, y: 0, width: 100, height: 20 },
  enclosingSourceSpan: { startByte: 0, endByte: 5 },
};
void legacy;
// @ts-expect-error onLink must be a callback
new FlowReaderView(element, { onLink: "javascript:bad" });
// @ts-expect-error no ambient navigation option
new FlowReaderView(element, { href: "https://example.com" });
