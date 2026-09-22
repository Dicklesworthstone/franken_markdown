// Compile-only public /book contract. This does not execute a WASM engine.
import { createBook, type BookFile, type BookSourceUpdate,
  type BookSourceUpdateOptions } from "./book.js";

const files: readonly BookFile[] = [{ path: "one.md", source: "# One" }];
const session = await createBook(files, { includeSources: [{ path: "part.md", source: "Shared" }] });
const options: BookSourceUpdateOptions = { expectedRevision: session.sourceRevision };
const result: BookSourceUpdate = session.updateSources([
  { path: "one.md", source: "# Revised" }, { path: "part.md", source: "Revised snippet" },
], options);
const revision: number = result.revision;
const indexes: readonly number[] = result.reparsedChapters;
session.updateSources(files);
session.renderPdf({ page: { size: "a4" } }).blob();
session.renderEpub().blob();
session.renderSite().blob();
// @ts-expect-error The result is synchronous, not a queued Promise.
result.then(() => {});
// @ts-expect-error Committed result counts are immutable.
result.changedSources = 99;
// @ts-expect-error Reparse indexes are immutable.
result.reparsedChapters.push(42);
// @ts-expect-error Revision cannot be set by the host.
session.sourceRevision = 0;
// @ts-expect-error Source replacements must be UTF-8 strings, not bytes.
session.updateSources([{ path: "one.md", source: new Uint8Array() }]);
// @ts-expect-error Revisions are numbers, never coerced strings.
session.updateSources(files, { expectedRevision: "0" });
// @ts-expect-error An update must not silently change render options.
session.updateSources(files, { font: "serif" });
// @ts-expect-error Reading order is fixed; source updates are not rename operations.
session.updateSources([{ path: "one.md", renameTo: "other.md" }]);
session.dispose();
void [revision, indexes];
