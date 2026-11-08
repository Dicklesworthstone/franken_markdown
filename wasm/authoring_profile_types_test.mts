// Compile-only public profile contract; runtime/native behavior has separate tests.
import { renderHtml, renderPdf, renderSvg, renderEpub, renderBookPdf, renderBookSite,
  renderInteractiveHtml, createRenderer } from "./franken_markdown.js";
import { renderDocumentBatch } from "./document-batch.js";
import { createBook, renderBookChapterPreview, type BookOptions } from "./book.js";
import { createBookWorker, createBookPublicationWorker } from "./book-worker.js";

const source = "Term\n: Definition";
await renderHtml(source, { profile: "commonmark-gfm", fontEmbedding: "full" });
await renderPdf(source, { profile: "gfm-plus", pdfA: "2b", baselineGridPt: 12 });
await renderSvg(source, { profile: "commonmark-gfm", maxWidthPt: 720 });
await renderEpub(source, { profile: "gfm-plus" });
const renderer = await createRenderer();
await renderer.renderHtml(source, { profile: "gfm-plus" });
for await (const _receipt of renderDocumentBatch([
  { format: "html", source, options: { profile: "commonmark-gfm" } },
  { source, targets: [{ format: "pdf", options: { profile: "gfm-plus" } }] },
])) { /* compile only */ }
// @ts-expect-error Profiles are canonical names without aliases.
await renderHtml(source, { profile: "gfm" });
// @ts-expect-error Profile admission does not coerce booleans.
await renderPdf(source, { profile: true });
// @ts-expect-error Lightweight interactive editing does not retain profiles.
await renderInteractiveHtml(source, { profile: "commonmark-gfm" });
await renderBookPdf([{ path: "one.md", source }], { profile: "gfm-plus" });
await renderBookSite([{ path: "one.md", source }], { profile: "commonmark-gfm" });

const bookFiles = [{ path: "one.md", source }];
const bookOptions: BookOptions = { profile: "commonmark-gfm", pdfA: "2b", baselineGridPt: 12 };
const book = await createBook(bookFiles, bookOptions);
const bookProfile: BookOptions["profile"] = book.renderPdf().profile;
const previewProfile: BookOptions["profile"] = (await renderBookChapterPreview(bookFiles, 0, bookOptions)).profile;
const worker = createBookWorker();
const workerProfile: BookOptions["profile"] = (await worker.render(bookFiles, "preview", bookOptions)).profile;
const publication = createBookPublicationWorker();
const edition = await publication.render(bookFiles, ["pdf", "epub", "site"], bookOptions);
const publishedProfile: BookOptions["profile"] = edition.outputs[0]?.profile;
// @ts-expect-error Book authoring profiles use the same strict canonical names.
await createBook(bookFiles, { profile: "gfm" });
void [bookProfile, previewProfile, workerProfile, publishedProfile];
book.dispose(); worker.dispose(); publication.dispose();
