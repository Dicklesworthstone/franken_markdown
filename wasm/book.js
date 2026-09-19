// Shares initialization and the WASM instance with the main renderer entry.
import { init } from "./franken_markdown.js";
import * as engine from "./pkg/franken_markdown.js";
import { createBookBindings } from "./book_session.mjs";

const bindings = createBookBindings(async () => {
  await init();
  return engine.FmdBook;
});

export const createBook = bindings.createBook;
export const renderBookPdf = bindings.renderBookPdf;
export const renderBookEpub = bindings.renderBookEpub;
export const renderBookSite = bindings.renderBookSite;
export const checkBookLinks = bindings.checkBookLinks;
export { parseBookLinkReport } from "./book_session.mjs";
