// An actual later layout acknowledgment must not invalidate an accepted image.
import test from "node:test";
import assert from "node:assert/strict";
import { FlowImageAssets } from "./flow-assets.js";
import { Session, png, image } from "./tests/flow_image_fixtures.mjs";

test("a subsequent reflow may advance layout before an earlier image acknowledgment is observed", async () => {
  const session = new Session(), original = session.provideAsset.bind(session), bitmap = image();
  session.provideAsset = async result => { const ack = original(result); session.reflow(); return ack; };
  const assets = new FlowImageAssets(session, { load: () => png(), decode: () => bitmap, timeoutMs: 0 });
  try {
    assert.equal((await assets.loadPending()).loaded, 1);
    assert.equal(assets.resolveImage({ requestId: "1", destination: "1.png", isResolved: true }, session.token), bitmap);
  } finally { assets.dispose(); }
});
