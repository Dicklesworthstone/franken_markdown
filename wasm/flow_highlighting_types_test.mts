import type { FlowSession, FlowToken } from "./flow.js";

declare const session: FlowSession;
const supported: boolean = session.supportsCodeHighlighting;
const enabled: boolean = session.codeHighlighting;
const token: FlowToken = session.setCodeHighlighting(true, session.token);
// @ts-expect-error strings are not boolean mode values
session.setCodeHighlighting("true", session.token);
// @ts-expect-error a layout token is mandatory even for no-op changes
session.setCodeHighlighting(false);
// @ts-expect-error numeric identities lose u64 precision
session.setCodeHighlighting(true, { revision: 1, layoutRevision: 1 });
// @ts-expect-error modes are changed transactionally, not by property assignment
session.codeHighlighting = false;
void [supported, enabled, token];
