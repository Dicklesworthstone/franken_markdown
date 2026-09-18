import { createBookWorker } from "../book-worker.js";
import { createBookControls } from "./book_controls.mjs";
const controls = createBookControls({ root: document, worker: createBookWorker(), confirm: text => window.confirm(text) });
window.addEventListener("pagehide", event => { if (event.persisted) controls.suspend(); else controls.dispose(); });
