import { createRenderer, extractFontCollectionFace, fontCollectionFaceCount, FontCollectionError,
  type FmdFontCollectionErrorCode, type FmdFontAsset } from "./franken_markdown.js";

const bytes = new Uint8Array([1, 2, 3]);
const counts: Promise<number>[] = [fontCollectionFaceCount(bytes), fontCollectionFaceCount(bytes.buffer),
  fontCollectionFaceCount(new DataView(bytes.buffer))];
const faces: Promise<Uint8Array>[] = [extractFontCollectionFace(bytes), extractFontCollectionFace(bytes, 1)];
void Promise.all(counts);
void Promise.all(faces);
void createRenderer().then(async renderer => {
  const count: number = await renderer.fontCollectionFaceCount(bytes);
  const asset: FmdFontAsset = { slot: "body-regular", bytes: await renderer.extractFontCollectionFace(bytes, count - 1) };
  return renderer.renderHtml("A B 日", { fontAssets: [asset] });
});
const error = new FontCollectionError("FONT_COLLECTION_FACE_INDEX", "Invalid selection");
const code: FmdFontCollectionErrorCode = error.code;
void code;
// @ts-expect-error Input bytes must be an ArrayBuffer or view, not an element list.
void fontCollectionFaceCount([1, 2, 3]);
// @ts-expect-error A face selector is a number, not a coerced form-control string.
void extractFontCollectionFace(bytes, "1");
// @ts-expect-error Collection failures use explicit stable error categories.
new FontCollectionError("MYSTERY_ERROR", "message");
