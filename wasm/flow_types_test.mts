// Compile-only contract test: tsc --noEmit --strict --target ES2022
// --module NodeNext --moduleResolution NodeNext wasm/flow_types_test.mts
import { createFlowSession, type FlowItem, type FlowToken, FlowError } from "./flow.js";
const session = await createFlowSession("# Guide", { font: "sans", viewportWidth: 360 });
const token: FlowToken = session.token;
session.edit(0, 0, "Intro\n\n", { expectedRevision: token.revision });
session.editBytes(0, 0, "", { expectedRevision: 1n, reuseAssets: false });
const page = session.snapshot({ glyphs: true, limit: 512 });
const item: FlowItem | undefined = page.items[0];
if (item?.kind === "text") {
  const id: string | undefined = item.fontRun?.fontId;
  if (id) session.fontBytes(id);
  session.selectText(item.index, 0, item.text.length, page);
}
session.reflow({ viewportWidth: 720 }, session.token);
session.hitTest(10, 10, page);
session.provideAsset({ requestId: "1", generation: session.revision, width: 10, height: 10 });
for (const snapshot of session.pages({ limit: 10 })) console.log(snapshot.items.length);
// @ts-expect-error Number cannot represent all document revisions.
session.edit(0, 0, "x", { expectedRevision: 1 });
// @ts-expect-error Geometry queries require the displayed snapshot token.
session.hitTest(0, 0);
// @ts-expect-error Byte arrays cannot be silently coerced into asset payloads.
session.provideAsset({ requestId: "1", generation: "1", width: 1, height: 1, bytes: [1, 2] });
// @ts-expect-error Unknown options must not be silently accepted.
await createFlowSession("x", { fontWeight: 400 });
const error = new FlowError("STALE_LAYOUT", "stale");
console.log(error.code);
session.dispose();
