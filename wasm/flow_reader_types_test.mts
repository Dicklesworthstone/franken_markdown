import {
  FlowReaderView,
  type ReadingFlowSession,
  readFlowDocument,
  sourceSpanToUtf16,
} from "@franken-suite/franken-markdown/flow-reader";
import type { FlowPageOptions, FlowReadingPage, FlowSession } from "./flow.js";

declare const sync: FlowSession;
declare const asyncSession: {
  readonly disposed: boolean;
  readonly token: FlowSession["token"];
  readingOrder(options?: FlowPageOptions): Promise<FlowReadingPage>;
};
declare const element: HTMLElement;
const sessions: ReadingFlowSession[] = [sync, asyncSession];
for (const session of sessions) {
  const document = await readFlowDocument(session, {
    token: { revision: 1n, layoutRevision: "1" },
    limits: { maxNodes: 200 },
  });
  const view = new FlowReaderView(element);
  view.render(document);
  const result = document.find("example", { asciiCaseInsensitive: true, maxMatches: 20 });
  for (const match of result.matches) {
    const copied: string = view.selectMatch(match);
    const location = view.focusNode(match.nodeIndex);
    const range: Readonly<{ start: number; end: number }> = sourceSpanToUtf16(
      sync.source,
      location.enclosingSourceSpan,
    );
    void copied;
    void range;
  }
  // @ts-expect-error identity inputs are never lossy Numbers
  await readFlowDocument(session, { token: { revision: 1, layoutRevision: "1" } });
  // @ts-expect-error snapshot text is immutable
  document.nodes[0].text = "mutated";
  // @ts-expect-error regexp searching is intentionally not part of this API
  document.find(/example/);
  view.dispose();
}
