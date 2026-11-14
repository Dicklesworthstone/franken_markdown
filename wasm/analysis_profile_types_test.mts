import { documentStats, accessibilityAudit, createRenderer,
  type FmdAnalysisOptions, type FmdAuthoringProfile } from "./franken_markdown.js";
const options: FmdAnalysisOptions = { profile: "commonmark-gfm" };
const stats = await documentStats("Term\n: Definition", options);
const audit = await accessibilityAudit("Term\n: Definition", { profile: "gfm-plus" });
const profile: FmdAuthoringProfile | undefined = stats.profile;
const renderer = await createRenderer();
await renderer.documentStats("source", options);
await renderer.accessibilityAudit("source", options);
void [profile, audit.profile];
// @ts-expect-error Analysis admits only canonical grammar names.
await documentStats("source", { profile: "gfm" });
// @ts-expect-error Source analysis grants no publication assets.
await accessibilityAudit("source", { profile: "gfm-plus", pdfImages: [] });
// @ts-expect-error Reports identify the grammar that ran.
stats.profile = "gfm-plus";
