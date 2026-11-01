import { createFlowSession, type FlowGlyphOutlines, type FlowPathItem,
  type FlowDisplayPathCommand, type FlowPathCommand, type FlowReadingNode } from "@franken-suite/franken-markdown/flow";
import {
  type CanvasFlowSession,
  FlowCanvasRenderer,
} from "@franken-suite/franken-markdown/flow-canvas";
import { createWorkerFlowSession } from "@franken-suite/franken-markdown/flow-worker";

const sync = await createFlowSession("# Guide");
const worker = await createWorkerFlowSession("# Guide");
const a: CanvasFlowSession = sync;
const b: CanvasFlowSession = worker;
const path: FlowGlyphOutlines = sync.glyphOutlines("1", [12, 14] as const);
const cubic: FlowDisplayPathCommand = ["C", 0, 1, 2, 3, 4, 5];
const mathPath: FlowPathItem = { kind: "path", index: 0, colorRole: "text", fill: [1, 0, 0, 1],
  bounds: { x: 0, y: 0, width: 10, height: 10 }, enclosingSourceSpan: { startByte: 0, endByte: 12 },
  commands: [["M", 0, 0], cubic, ["Z"]] };
const mathReading: FlowReadingNode = { role: "math", text: "\\frac{x}{y}", bounds: mathPath.bounds,
  enclosingSourceSpan: mathPath.enclosingSourceSpan, children: [] };
// @ts-expect-error Display paths do not expand the narrower glyph outline protocol.
const glyphCubic: FlowPathCommand = cubic;
// @ts-expect-error Display commands cannot be mutated through the public type.
mathPath.commands[0] = ["Z"];
// @ts-expect-error Explicit color channels are immutable.
mathPath.fill![0] = 0;
const remote: FlowGlyphOutlines = await worker.glyphOutlines(1n, new Uint16Array([12]));
const paint = new FlowCanvasRenderer(new OffscreenCanvas(1, 1), {
  limits: { maxCachedGlyphs: 100 },
});
await paint.render(a, { width: 400, height: 600, pixelRatio: 2 });
const frame = await paint.render(b, { token: worker.token, resolveImage: async () => null });
const hit = await paint.hitTest(10, 10);
await paint.render(b, { selection: { ...frame, rectangles: [] } });
// @ts-expect-error Font IDs never pass through JS numbers.
sync.glyphOutlines(123, [1]);
// @ts-expect-error Unknown Canvas options must not silently degrade fidelity.
new FlowCanvasRenderer(new OffscreenCanvas(1, 1), { fillText: true });
// @ts-expect-error Complete immutable token required for explicit selection.
await paint.render(b, { selection: { rectangles: [] } });
void [path, remote, hit, mathReading, glyphCubic];
paint.dispose();
worker.dispose();
sync.dispose();
