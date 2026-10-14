// Explicit source-host adapter. No worker or Markdown/PDF renderer is invoked;
// collection, resource ownership, profile validation and UI are production code.
import { createBookCollection, readBookProject, readPortableBookProject } from "../demo/book_collection.mjs";
import { createBookPublicationControls } from "../demo/book_pdf_controls.mjs";

export function startPublicationFixture(root = document) {
  const collection = createBookCollection(), sourceListeners = new Set();
  collection.append({ chapters: [{ path: "book.md", source: "\ufeff# Unicode book\r\n日本語 😀\r\n" }],
    includeSources: [{ path: "include.txt", source: "keep\r\nexact" }],
    images: [{ destination: "assets/logo.png", bytes: new Uint8Array([1, 2, 3]) }] });
  // The image is a byte-store fixture, not a PNG decoding test.
  let busy = false, changes = 0;
  const controls = {
    get sourceBusy() { return busy; },
    captureProject() { throw Error("Profile authoring must not capture source"); },
    subscribeSourceState(fn) { sourceListeners.add(fn); return () => sourceListeners.delete(fn); },
  };
  const ui = createBookPublicationControls({ root, controls, collection });
  const unsubscribe = collection.subscribe(() => changes++);
  return Object.freeze({ collection, ui,
    get changes() { return changes; },
    setBusy(value) { busy = value; for (const fn of sourceListeners) fn(); },
    async roundTrip(portable = false) {
      if (portable) {
        const download = await collection.portableDownload();
        const review = await readPortableBookProject(new File([download.blob], "book.bundle.json"));
        collection.replacePortableProject(review, collection.revision, { authorizeResources: true });
      } else {
        const input = new File([collection.projectDownload().blob], "book.json");
        collection.replaceProject(await readBookProject(input));
      }
    },
    close() { ui.dispose(); unsubscribe(); collection.dispose(); },
  });
}
