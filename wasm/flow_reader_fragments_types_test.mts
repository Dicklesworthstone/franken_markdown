import type { FlowReadingNode } from "./flow.js";
import type { FlowReadingEntry, FlowReadingLocation, ReadingFlowSession } from "./flow-reader.js";
import { readFlowDocument } from "./flow-reader.js";

declare const session: ReadingFlowSession;
const document = await readFlowDocument(session, { limits: { maxAnchorUnits: 4096 } });
const location: FlowReadingLocation | null = document.locateFragment("#fmd:note:1");
const entry: FlowReadingEntry = document.nodes[0];
const id: string | null = entry.anchorId;
const bytes: number = document.anchorUnits;
declare const raw: FlowReadingNode;
const wireId: string | null | undefined = raw.anchorId;
void [location, id, bytes, wireId];
// @ts-expect-error Fragment targets are strings, not source offsets.
document.locateFragment(42);
// @ts-expect-error Snapshot entries cannot be mutated by the host.
entry.anchorId = "changed";
// @ts-expect-error Retention budgets are numeric.
await readFlowDocument(session, { limits: { maxAnchorUnits: "4096" } });
