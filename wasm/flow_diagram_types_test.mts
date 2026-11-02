import type { FlowPathItem, FlowPathStroke, FlowTextItem, FlowReadingNode } from "./flow.js";
import type { FlowReadingEntry } from "./flow-reader.js";

const stroke: FlowPathStroke = {
  color: [0.1, 0.2, 0.3, 0.5], width: 2, dash: [4, 2], dashOffset: -1,
  cap: "square", join: "miter", miterLimit: 4,
};
const path: FlowPathItem = {
  kind: "path", index: 0, bounds: { x: 0, y: 0, width: 30, height: 30 },
  enclosingSourceSpan: { startByte: 0, endByte: 100 }, colorRole: "diagram", fill: null,
  stroke, commands: [["M", 10, 10], ["L", 20, 20]],
};
const legacy: FlowPathItem = { ...path, stroke: null, commands: [["M", 10, 10], ["L", 20, 20], ["Z"]] };
const label: FlowTextItem = { ...path, kind: "text", text: "Label", fontSize: 14, fontRun: null,
  fill: [0.3, 0.2, 0.1, 1], baselineY: 20 };
const transcript: FlowReadingNode = { role: "diagram", text: "flowchart TD\nA-->B\n",
  bounds: path.bounds, enclosingSourceSpan: path.enclosingSourceSpan, children: [] };
function inspect(entry: FlowReadingEntry): string | undefined {
  return entry.role === "diagram" ? entry.text : undefined;
}
void [legacy, label, transcript, inspect];
// @ts-expect-error: unsupported cap spelling is not silently coerced.
const invalid: FlowPathStroke = { ...stroke, cap: "flat" };
void invalid;
