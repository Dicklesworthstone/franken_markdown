import { FlowReaderView, readFlowDocument } from "./flow-reader.js";
import type { ReadingFlowSession } from "./flow-reader.js";
import type { FlowReadingListItem, FlowReadingNode } from "./flow.js";
declare const session: ReadingFlowSession;
declare const container: HTMLElement;
const doc = await readFlowDocument(session, { limits: { maxListEntries: 1000 } });
const count: number = doc.listEntryCount;
const path: readonly FlowReadingListItem[] | null = doc.nodes[0].listPath;
if (path?.length) {
  const ordinal: string = path[0].start;
  const identity: string = path[0].listId;
  const task: boolean | null = path[0].task;
  void [ordinal, identity, task];
  // @ts-expect-error immutable snapshot metadata
  path[0].itemIndex = 5;
  // @ts-expect-error immutable ancestry arrays
  path.push(path[0]);
}
const old: FlowReadingNode = { role: "list-item", text: "legacy", children: [],
  bounds: { x: 0, y: 0, width: 100, height: 20 }, enclosingSourceSpan: { startByte: 0, endByte: 5 } };
const current: FlowReadingNode = { ...old, listPath: [{ listId: "1", ordered: true, start: "7", itemIndex: 0, task: null }] };
new FlowReaderView(container).render(doc);
void [old, current, count];
// @ts-expect-error raw wire ancestry is optional, not nullable
const invalid: FlowReadingNode = { ...old, listPath: null };
// @ts-expect-error ordinals never pass through JavaScript Number
const rounded: FlowReadingListItem = { listId: "1", ordered: true, start: 9007199254740993, itemIndex: 0, task: null };
void [invalid, rounded];
