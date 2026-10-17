import { installDocumentInspector } from "./document_inspector.mjs";

installDocumentInspector(self, () => import("./franken_markdown.js"));
