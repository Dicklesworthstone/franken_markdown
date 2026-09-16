import { createBook, renderBookEpub, type BookOutput } from "../book.js";
const files = [{ path: "index.md", source: "# Home" }] as const;
const session = await createBook(files, {
  title: "Book", fontScale: 1.125, pageNumbers: true,
  images: [{ destination: "image.png", bytes: new DataView(new ArrayBuffer(1)) }]
});
try {
  const result: BookOutput = session.renderPdf();
  const blob: Blob = result.blob();
  const bytes: Uint8Array = result.bytes;
  const name: string = result.filename("manual");
  void [blob, bytes, name];
} finally { session.dispose(); }
const epub: BookOutput = await renderBookEpub(files);
void epub;
// @ts-expect-error String scale presets are intentionally not part of this API.
createBook(files, { fontScale: "large" });
// @ts-expect-error A source string is required for every chapter.
createBook([{ path: "x.md" }]);
// @ts-expect-error Slot names are constrained.
session.setFont("unknown", new Uint8Array([1]));
