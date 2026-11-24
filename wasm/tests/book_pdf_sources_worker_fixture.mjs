// Explicit FmdBook ABI double, not an include expander or PDF renderer.
// The worker executes the production book session and both transport endpoints.
import { parentPort } from "node:worker_threads";
import { createBookBindings } from "../book_session.mjs";
import { installBookWorker } from "../book_worker.mjs";

const encode = value => new TextEncoder().encode(value);
class Book {
  constructor(paths, sources) {
    this.capture = { paths, sources, includePaths: [], includeSources: [], expandIncludes: false,
      images: [], fonts: [] };
  }
  static fromSources(paths, sources, includePaths, includeSources) {
    const book = new Book(paths, sources);
    Object.assign(book.capture, { includePaths, includeSources, expandIncludes: true });
    return book;
  }
  static newWithProfile(paths, sources, profile) {
    const book = new Book(paths, sources);
    book.setAuthoringProfile(profile);
    return book;
  }
  static fromSourcesProfiled(paths, sources, includePaths, includeSources, profile) {
    const book = Book.fromSources(paths, sources, includePaths, includeSources);
    book.setAuthoringProfile(profile);
    return book;
  }
  setAuthoringProfile(profile) { this.capture.profile = profile; }
  setMetadata(title, author, lang) { this.capture.metadata = { title, author, lang }; }
  setCustomCss() {}
  setTheme() {}
  setNavigation() {}
  setFontScale() {}
  setImage(destination, bytes) { this.capture.images.push({ destination, bytes: [...bytes] }); }
  setFont(slot, bytes) { this.capture.fonts.push({ slot, bytes: [...bytes] }); }
  setFontWeight() {}
  get chapterCount() { return this.capture.paths.length; }
  get sourceLength() {
    return [...this.capture.sources, ...this.capture.includeSources]
      .reduce((total, source) => total + encode(source).length, 0);
  }
  renderPdf() { return encode(JSON.stringify(this.capture)); }
  free() {}
}

const engine = createBookBindings(async () => Book);
installBookWorker({
  addEventListener(type, callback) {
    if (type !== "message") throw new Error("Unexpected worker fixture event");
    parentPort.on(type, data => callback({ data }));
  },
  postMessage(data, transfer) { parentPort.postMessage(data, transfer); },
}, engine);
