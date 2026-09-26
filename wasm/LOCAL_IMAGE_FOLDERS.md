# Existing Markdown folders in the Canvas editor

The image panel in `demo/flow-canvas.html` now includes **Load nested images from
a local folder**. Open your Markdown through the existing source controls first,
then choose its folder, enter its path within that folder, and press **Use this
image folder**. Selecting a folder alone does not replace the active image grant.

For this tree:

```text
project/
  docs/guide.md
  images/plot.png
```

Select `project`, enter `docs/guide.md`, and keep the original Markdown reference
`![Plot](../images/plot.png)`. Do not include `project/` in the path field. Leave
the field blank for a Markdown file directly inside the selected folder. This
field sets image resolution; it does not open, read, rename, or rewrite Markdown.

Images in different directories may have identical basenames. Spaces, Unicode,
parentheses and literal percent characters use unambiguous percent-encoded URL
segments. Insertion generates escaped references relative to the applied document
path. Editing that field has no effect until Apply, so an active preview never
silently changes its resource base.

## Ownership and publication

Only selected `.png`, `.jpg` and `.jpeg` File objects are retained by the loader.
Nonimage file contents are never read by it. The browser picker itself enumerates
the selected folder, so choose a small document folder rather than a home directory.
Relative `..` references can ascend only inside that explicitly selected root.
Absolute paths, network URLs, queries, fragments, encoded separators/dot segments,
malformed encodings and unselected files are refused. There is no network fallback.

The existing image manager still validates the actual raster container and pixel
budgets. File extensions do not prove valid image contents. Its encoded retention
path feeds the existing document exporter, not a screenshot or replacement parser.
Native export still rejects unresolved images. No Rust/WASM API change is required
by this JavaScript feature; assemble these updated demo modules with the matching
existing package. SVG/GIF/WebP and other decoder-profile exclusions remain unchanged.

Applying a new grant invalidates prepared downloads and restarts the preview to
clear old pixels, bitmaps and native asset bytes. Invalid directory admission keeps
the previous grant. A renderer restart failure stops old presentation instead of
claiming that authorization rolled back. Opening/restoring another source document,
explicit revocation, or page suspension drops the selected folder and base. Returning
through the back/forward cache requires selecting files again; late picker/source
events during suspension cannot restart a worker. Markdown text stays untouched.

The flat image picker and its exact-name aliases remain available. Browsers without
directory input support disable only the new folder controls. No folder handle,
permission, image bytes or base path is saved in drafts or persistent storage.

## Bounds and checks

Directory admission permits at most 4096 file entries, 128 PNG/JPEG images, 8 MiB
per image and 32 MiB of image payloads. Paths have bounded depth/length, and both
path metadata and generated reference text have separate 1 MiB limits. Admission
reads metadata only; payload reads occur on demand with cancellation and the image
manager's byte limit. These are not total browser/codec memory ceilings.

```sh
node --test wasm/tests/local_image_sources.test.mjs wasm/tests/local_image_directory.test.mjs wasm/tests/local_image_directory_controls.test.mjs
python wasm/tests/run_local_image_directory.py --chromium /usr/bin/chromium
```

The executed Node suites have 32 passing tests: five original flat-picker cases,
15 directory-loader cases and 12 controls/entrypoint cases. File/Blob, path resolution
and raster admission are real; the entrypoint tests explicitly double the DOM,
worker, painter and surrounding controllers. The Chromium runner has 15 passing
checks using actual directory-input FileLists, file reads and DOM controls with
network requests blocked. It does not exercise the operating system's picker dialog,
image decoding, the full worker, or generated Rust/WASM rendering. Those remain
separate platform/package acceptance requirements.
