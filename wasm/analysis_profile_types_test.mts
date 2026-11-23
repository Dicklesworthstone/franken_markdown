import { documentStats, accessibilityAudit, searchIndex, semanticDiff, renderSemanticDiff, createRenderer,
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

const index = await searchIndex("Term\n: Definition", options);
const diff = await semanticDiff("before", "after", { oldName: "Before Ω", ...options });
const review = await renderSemanticDiff("before", "after", { profile: "gfm-plus" });
const diffFormat: "diff-html" = review.format;
const searchProfile: FmdAuthoringProfile | undefined = index.profile;
const diffProfile: FmdAuthoringProfile | undefined = diff.profile;
const reviewProfile: FmdAuthoringProfile | undefined = review.profile;
await renderer.searchIndex("Term\n: Definition", options);
await renderer.semanticDiff("before", "after", options);
await renderer.renderSemanticDiff("before", "after", options);
void [diffFormat, searchProfile, diffProfile, reviewProfile];
// @ts-expect-error Search uses only source grammar, not rendering grants.
await searchIndex("source", { profile: "gfm-plus", pdfImages: [] });
// @ts-expect-error Revision review requires a canonical grammar name.
await semanticDiff("before", "after", { profile: "gfm" });
// @ts-expect-error Comparison HTML cannot change renderer resources.
await renderSemanticDiff("before", "after", { fontAssets: [] });
// @ts-expect-error The receipt records the native grammar that ran.
review.profile = "commonmark-gfm";
