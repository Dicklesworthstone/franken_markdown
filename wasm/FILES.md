# Editable Markdown files in the browser

Open `demo/flow-canvas.html` from the assembled package. The **Direct Markdown
file saving** panel sits beside the existing original-source download controls.
It is independent of the renderer: a failed preview does not prevent saving or
recovering source.

**Open editable Markdown** connects one explicitly selected file. **Save current
file** writes the current source back to that file. **Save Markdown as…** chooses
a destination, confirms replacement of nonempty contents, and connects the new
file only after the write is verified. Ctrl/Cmd+S saves; Shift+Ctrl/Cmd+S chooses a
destination. A renamed source filename requires Save as rather than silently
writing the old file.

These controls require a secure page and the browser's file-picker APIs. Their
availability is detected separately; unsupported browsers retain the ordinary
file input and **Prepare Markdown** / **Download** workflow. In those browsers,
the Save shortcut prepares a source download instead. Download preparation does
not prove that a file was saved, so it never marks a disk baseline as saved.
Native pickers and permission requests are initiated from the click/key action.
No automatic saves to user-selected files occur.

## Source fidelity and document ownership

Direct file operations admit `.md`, `.markdown`, and `.txt` files up to 4 MiB.
Names and source use the existing validation code. Malformed UTF-8 files,
unpaired editor surrogates, over-budget source, and invalid names are refused
without discarding editor text. File type validation is deliberately conservative:
Save as will not overwrite an HTML, script, binary, or unsupported-encoding file.

The original UTF-8 BOM and line endings survive while the imported textarea view
is unchanged. Edited source follows the existing textarea's LF convention;
returning to the exact imported view restores the original source bytes. Writes
use transparent Blob line endings, not operating-system newline conversion.

Saving does not replace the source document, clear undo/redo, revoke image
access, or restart the renderer. Successful Save as changes only the filename
and the file association. Open and Reload use the established document-replacement
transaction: they explicitly confirm replacing edits, clear document-local undo
history, and revoke the previous document's image access. Importing through the
ordinary file input or restoring a browser draft disconnects the previous file;
a filename alone never grants permission to overwrite it.

## External changes and uncertain saves

Each connected file has a private exact-source baseline. Size and modification
time alone are not used to decide whether a file changed. Save compares current
disk contents with that baseline before opening a writer, after opening it, and
after staging source but before closing it. Writes request an exclusive browser
writer and a replacement stream rather than appending to previous contents.
Every pre-close asynchronous step also checks the editor revision and document
identity. Changed source, imports, text composition, disconnection, or suspension
abort the staged writer instead of committing an older captured revision.

Only a completed close followed by matching read-back updates the saved baseline.
Edits made while close is already underway remain in the editor and remain
unsaved. A failed close or unverifiable read-back is shown as **uncertain**, not
saved; current-file retries are paused rather than automatically repeated.

When a disk conflict or uncertain outcome occurs, **Keep editing** leaves edits
untouched. **Prepare recovery download** preserves a separate source copy through
the existing download link. Save as can write a different file. Selecting another
handle for the same file cannot bypass a conflict: actual file-entry identity is
checked. **Reload file version** is an explicit discard/reopen operation, with a
second disk comparison after confirmation so a changing file is not installed
from an outdated read.

These checks are **not an operating-system-wide compare-and-swap**. Another
application can write between the final comparison and close, and exclusive
writer behavior depends on browser support. A successful read-back is evidence
about that completed operation, not continuous monitoring or a guarantee against
later external writes. Keep important work backed up, especially when another
application is editing the same file. A native Save as picker may create an empty
file entry before subsequent confirmation or validation is cancelled; refusing a
write means no source bytes were committed, not that the picker created no entry.

## Permissions and lifecycle

File handles and disk baselines remain private to the current page session.
They are never included in drafts, worker messages, source downloads, or local
storage. **Disconnect file** drops the application's association; it does not
revoke any site permission retained by the browser. Manage those permissions in
the browser's settings.

Page suspension, including the back/forward-cache lifecycle, drops file access.
Only the existing in-memory source may be retained when returning; reopening is
required to reconnect a file. No unload save is attempted or claimed. A
before-unload warning is attached after edits when unsaved work or a pending or
uncertain write exists, and removed when no longer needed. Browser leave-page
warnings are best-effort and are not a backup mechanism.

The separate local-draft feature remains opt-in and source-only. Saving a draft
is not saving a connected disk file, and saving a disk file does not imply that a
browser draft was updated. No source or file bytes are uploaded by these tools.

## Verification

```sh
node --test wasm/tests/flow_file_*.test.mjs
```

The Node suites exercise the production file session, panel, original-source
controller, and undo/redo controller. File-system, permission, and DOM doubles are
explicit; native Node File, Blob, UTF-8 codecs and Blob URLs are used. Tests cover
same-size external edits, staged-write aborts, picker/confirmation races,
same-entry Save as conflicts, newer edits during close, uncertain saves,
composition, restoration, downloads and disposal. Package tests check the actual
entrypoint, manifest and both assemblers; they do not substitute for a generated
WASM package build.

Native file-picker UX, user-visible filesystem permissions, real writable-stream
locking, actual back/forward caching, assistive-technology behavior, and
Rust/WASM rendering require separate platform acceptance. A local Chromium
attempt was blocked by the environment's managed URL policy before the test page
loaded; it provides no passing native-browser evidence for this change.
