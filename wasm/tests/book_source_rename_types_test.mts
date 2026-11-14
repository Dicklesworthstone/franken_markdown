import { renameBookSource, type BookFile, type BookSourceRenameOptions, type BookSourceRenameOutput } from "../book.js";
import { createBookWorker } from "../book-worker.js";

const files: readonly BookFile[] = [{ path: "one.md", source: "# One" }];
const sourceOnly: BookSourceRenameOptions = { includeSources: [{ path: "part.txt", source: "Text" }],
  profile: "commonmark-gfm", imageDestinations: ["diagram.svg", "guide/diagram.svg"] as const };
const output: Promise<BookSourceRenameOutput> = renameBookSource(files, "one.md", "guide/one.md", sourceOnly);
const worker = createBookWorker();
const isolated: Promise<BookSourceRenameOutput> = worker.renameSource(files, "one.md", "guide/one.md", sourceOnly,
  { signal: new AbortController().signal });
void output; void isolated;
output.then(result => {
  const format: "book-source-rename" = result.format; void format;
  result.report.files[0]?.source;
  // @ts-expect-error Reports cannot be mutated before application.
  result.report.files[0].path = "changed.md";
  // @ts-expect-error Reports do not grant resource authority.
  result.report.images;
});
// @ts-expect-error Source refactoring does not accept image/resource bytes.
renameBookSource(files, "one.md", "new.md", { images: [] });
// @ts-expect-error Image metadata is an immutable string inventory, not assets.
worker.renameSource(files, "one.md", "new.md", { imageDestinations: [{ destination: "diagram.svg", bytes: new Uint8Array() }] });
// @ts-expect-error The inventory cannot be reassigned through the options type.
sourceOnly.imageDestinations = [];
// @ts-expect-error The inventory's strings cannot be overwritten.
sourceOnly.imageDestinations[0] = "replacement.svg";
// @ts-expect-error Authoring dialect is restricted to supported public values.
worker.renameSource(files, "one.md", "new.md", { profile: "markdown" });
// @ts-expect-error Source rename is a separate operation with required endpoints.
worker.render(files, "source-rename");
