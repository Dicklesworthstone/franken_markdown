# FrankenMarkdown for Apple Platforms

Status: implementation plan plus current delivery boundary

## 1. Product promise

FrankenMarkdown is a private, offline document forge for iPhone, iPad, and Mac. A user writes or imports Markdown once, watches the same Rust engine create a live HTML reading view, and exports a deterministic PDF or self-contained HTML file without an account, telemetry, uploads, or a server.

The app must feel like a first-class Apple document tool rather than a website placed inside a window. SwiftUI owns navigation, editing, documents, settings, sharing, keyboard commands, accessibility, state restoration, and platform adaptation. The existing browser package remains the single renderer implementation: a local `WKWebView` hosts the exact tracked Rust/WASM package and is never allowed to navigate to the network.

The visual direction is a premium “living document press”: restrained black glass and native materials around an emerald/amber laboratory core, the suite’s friendly monster as guide, and subtle typography/press mechanics. Spectacle belongs in truthful render feedback and transitions; controls remain immediately understandable.

## 2. Non-negotiable constraints

- Entirely on device. No model download, cloud API, analytics SDK, tracking, ads, login, or remote fonts.
- One rendering implementation. The app bundles `wasm/franken_markdown.js` plus the generated `wasm-bindgen` package; Swift never reimplements Markdown or PDF layout.
- Raw HTML is off by default and clearly marked as a document trust setting.
- Imported image and font bytes are host supplied to the existing renderer API; the renderer receives no filesystem or network access.
- User documents live in normal document storage or the app container. Temporary exports are removed by normal OS lifecycle, not by destructive repository scripts.
- Render status is honest. The bridge reports measured initialization, core render, bridge transfer, and total durations. It does not invent internal percentages that the ABI cannot observe.
- The UI respects Dynamic Type, VoiceOver, keyboard navigation, Reduce Motion, Reduce Transparency, high contrast, and sufficient hit targets.
- iPhone, iPad, and Mac have intentionally different compositions described below.

## 3. Engine boundary

### 3.1 Shippable renderer bundle

The canonical package is assembled by `scripts/check-wasm-package.sh` into `target/fmd-checks/wasm-package/` and contains:

- `franken_markdown.js`, the ergonomic API wrapper;
- `pkg/franken_markdown.js`, generated `wasm-bindgen` glue;
- `pkg/franken_markdown_bg.wasm`, the Rust core;
- `franken_markdown.d.ts`, used as the bridge contract; and
- the package README/license attribution.

`ios/sync-renderer.sh` will copy that verified package into `ios/Renderer/` without touching Rust source. A manifest records SHA-256 values for the wrapper and WASM binary. Release verification re-runs the package gate, refreshes the bundle, and rejects a mismatched manifest.

### 3.2 Offline resource delivery

`FrankenResourceSchemeHandler` serves a strict allow-list under `frankenmd://bundle/` from application resources. It assigns explicit MIME types for HTML, JavaScript, JSON, CSS, and WebAssembly, rejects path traversal, and has no fallback to HTTP/HTTPS. The web view also rejects every navigation outside that scheme.

The renderer page has no editor, toolbar, or product UI. It is a small render surface and bridge:

1. load the WASM module;
2. accept versioned commands from Swift;
3. render HTML into a sandboxed preview frame;
4. return diagnostics and measured timings;
5. transfer exported HTML/PDF bytes in ordered, checksummed chunks; and
6. report structured failures.

The native request envelope includes a monotonically increasing request ID. Results for superseded requests are ignored. A 180 ms debounce prevents rendering every keystroke while preserving a live feel.

### 3.3 Binary transfer

HTML preview remains inside the render process and does not cross the Swift bridge. Export bytes use a bounded chunk protocol:

- `begin`: request ID, MIME type, filename, byte length, SHA-256 supplied by JavaScript when available;
- `chunk`: zero-based index and base64 payload of at most 64 KiB raw data;
- `end`: chunk count and measured transfer time; and
- `cancel`: release the JavaScript buffer for an obsolete request.

Swift assembles chunks in order with a maximum document/export budget, writes to an application-temporary URL, validates byte length, and publishes the file only after completion. This avoids a single giant property-list message and gives export a real, measurable progress fraction.

## 4. Native document model

`MarkdownDocument` stores UTF-8 Markdown plus lightweight per-document presentation settings. The source file remains plain Markdown; app-only settings are stored in scene/document state, not injected into the user’s text.

Supported inputs:

- `.md`, `.markdown`, and UTF-8 plain text from Files;
- text shared from another app;
- drag and drop on iPad and Mac;
- image attachments chosen from Files or Photos and mapped to their Markdown destinations; and
- optional user-supplied TrueType fonts mapped to the renderer’s five documented slots.

Supported outputs:

- deterministic PDF;
- self-contained HTML;
- source Markdown;
- Copy Rendered Text where the HTML document can be safely converted to attributed/plain text; and
- system ShareLink / share sheet for every exported artifact.

Document autosave uses `FileDocument` and normal conflict handling. Untitled work is state-restored. Destructive replacement is always explicit and undoable through the editor’s undo manager.

The current single-window shell now provides both crash-safe active-draft recovery and real source-file ownership. It keeps security-scoped current-file identity, preserves an explicit UTF-8 byte-order mark, saves in place through coordinated atomic replacement, refuses to overwrite an external edit, offers Save a Copy, and persists at most six filename/bookmark recents without document contents. It stores bounded, versioned recovery source plus safe presentation settings in Application Support using atomic writes and complete file protection, excludes the recovery cache from backup, and refuses malformed or oversized state. Raw-HTML trust is intentionally never restored across launches or document adoption.

Full `DocumentGroup` browser ownership, automatic current-document restoration, document autosave, and multiwindow support remain open. The source-complete document-session tranche is tracked by Bead `br-best-in-class-markdown-renderer-fmd-agent-ergonomics-commonma-3ady`; Xcode, Simulator, and executable DSR evidence must be added only after the storage guard admits those lanes.

## 5. Information architecture

### 5.1 Shared surfaces

- **Forge**: source editor and rendered result.
- **Inspector**: typography, dark-mode behavior, title/author, page numbers, code line numbers, raw HTML trust, images, and fonts.
- **Diagnostics**: source-span warnings and errors; selecting one focuses the matching source range.
- **Export**: PDF/HTML/source destinations, file naming, measured result size, Quick Look, and ShareLink.
- **Library**: recent documents, pinned examples, import, and recovery of state-restored drafts.

The primary action is always Render/Export, not onboarding prose. Help and privacy details are concise sheets available from an information menu.

### 5.2 iPhone

- A compact top bar shows document title, saved state, and a quiet monster status mark.
- A native segmented control switches **Write**, **Preview**, and **Inspect**. The currently useful surface gets the whole screen.
- The editor uses a native text view wrapper for reliable selection, find/replace, undo, autocorrection control, line/column display, and external-keyboard commands.
- Preview is edge-to-edge inside one rounded material frame with a small measured render footer.
- Export is a bottom sheet with large format cards and Quick Look.
- Toolbars collapse while typing and reappear on scroll/tap; keyboard dismissal follows normal scroll/tap behavior.

### 5.3 iPad

- Regular width defaults to a two-column adjustable split: source on the left, live preview on the right.
- Inspector and diagnostics live in a trailing inspector rather than covering the document.
- Stage Manager and every orientation are supported; the layout falls back to the iPhone segmented composition at narrow split widths.
- Pointer hover, drag/drop, Scribble, hardware-keyboard shortcuts, command discovery, multiwindow documents, and contextual menus are first-class.
- Export previews use a centered resizable sheet or a secondary window, not a phone-width card stretched across the display.

### 5.4 Mac

- Mac Catalyst uses native design metrics and a three-pane `NavigationSplitView`: library, editor/preview workspace, inspector.
- The editor/preview divider is draggable and remembers its position.
- File, Edit, View, Format, Render, and Window commands expose expected shortcuts: New, Open, Save, Save As, Find, Replace, Render (`⌘R`), Export PDF (`⇧⌘E`), toggle preview (`⌥⌘P`), and toggle inspector (`⌥⌘I`).
- Multiple document windows, menu validation, right-click actions, drag/drop, titlebar toolbar items, full-screen preview, and standard window restoration are supported.
- The Mac app does not use iPhone-only sheets, giant touch buttons, or portrait max-width assumptions.

## 6. Spectacular but truthful render experience

`DocumentReactorView` is the suite’s domain-specific waiting surface. It resembles a compact glass-and-brass printing reactor with five stations:

`SOURCE → AST → THEME → TYPESET → HTML/PDF`

The ABI exposes one combined Rust core call, so during that call the middle stations are shown as a single energized pipeline, not as fake completed percentages. The caption says “Rust core active: parse · theme · layout · render.” Before and after the call, preparation and bridge/export stages advance from real events. On completion, the result sheet shows actual elapsed time, source bytes, output bytes, diagnostic count, and output hash prefix.

The animation includes a parchment ribbon, typographic glyph particles derived from the user’s source, warm press sparks, and a gentle success stamp. It uses `TimelineView`/`Canvas`, caps its frame rate under Low Power Mode or serious thermal state, and becomes a static accessible pipeline with textual status under Reduce Motion. It never blocks the editor or steals focus.

Haptics are reserved for explicit render start, success, warning, and failure. Sounds are off by default.

## 7. Shared FrankenSuite design system

The app uses the suite component vocabulary while tailoring the instrument to documents:

- `Lab` semantic colors and spacing, with green as success/primary, amber for PDF/press heat, cyan for HTML/live state, red only for actionable errors;
- `LaboratoryBackground` with restrained circuit filaments and native material fallbacks;
- `LabPanel`, `LabLabel`, `PrimaryButtonStyle`, `GhostButtonStyle`, `StatusLine`, and `LabProgressBar`;
- `MonsterStatusMark` with a document-press instrument;
- native `.regularMaterial`/`.ultraThinMaterial` when supported, plus opaque high-contrast fallbacks;
- SF Symbols for controls, never generated pictograms that compete with platform semantics; and
- rounded geometry and typography calibrated separately for pointer and touch environments.

The generated app icon master depicts the friendly suite monster operating a glowing Markdown-to-HTML/PDF press. It has no words or baked-in rounded corners. The website illustration is used as optional onboarding/empty-state art with responsive crops and an accessible description.

## 8. Apple platform integration

- **Document browser and Files**: open/create/edit standard Markdown documents in place.
- **Share extension**: accept text or a single Markdown/plain-text file, place a versioned import request in the App Group, and open the document forge.
- **Widgets**: a small “New document” launcher and medium recent-document/status widget. No private document contents appear unless the user explicitly enables previews.
- **App Intents / Shortcuts**: New Markdown Document, Open Markdown Forge, and Open Recent Document. Rendering itself opens the app because the WASM render surface is foreground-owned.
- **Spotlight**: opt-in indexing of file title, type, and modification date only; never body text by default.
- **Handoff / NSUserActivity**: continue the same file and selected source range between the user’s devices when the file itself is available through their chosen document provider.
- **Quick Look**: preview generated PDF/HTML artifacts before sharing.
- **Printing**: print the generated PDF through the system print controller.
- **Dynamic Island / Live Activities**: intentionally omitted for ordinary sub-second renders. If future measured real documents regularly exceed several seconds while background execution is valid, add a truthful activity then; novelty alone is not sufficient.

## 9. Privacy, security, and App Review posture

- `PrivacyInfo.xcprivacy` declares no collected data and no tracking domains.
- App Privacy in App Store Connect is “Data Not Collected.”
- Review notes explicitly state that source, attachments, fonts, previews, and exports remain on device and the renderer has no network capability.
- The app transport/security configuration does not grant arbitrary loads.
- The scheme handler canonicalizes every path, rejects `..`, and serves only a compile-time allow-list.
- External links in rendered Markdown are disabled inside preview by default. When enabled, tapping presents the destination and opens it through the system only after a user gesture.
- Raw HTML remains off by default, is labeled as content trust rather than privacy consent, and is sandboxed away from native message handlers beyond the narrow render protocol.
- Maximum source, attachment, font, diagnostic, and export sizes are enforced before allocation.
- Structured bridge messages are schema/version validated and unknown commands are rejected.

## 10. Accessibility and localization

- All editing and navigation remain usable with VoiceOver, Voice Control, Full Keyboard Access, Switch Control, and Dynamic Type.
- The reactor exposes one concise status plus optional details instead of making every decorative particle accessible.
- Diagnostics announce severity, line, column, and message; selection jumps preserve VoiceOver focus.
- Color is never the only status indicator.
- Reduce Motion removes particle travel, parallax, pulse, and automatic camera movement.
- Strings are centralized for localization; code, filenames, and Markdown remain monospaced only where semantically appropriate.

## 11. Targets and identifiers

Planned source of truth: `ios/project.yml` generated with XcodeGen.

- App: `com.frankenmarkdown.FrankenMarkdown`
- Widget: `com.frankenmarkdown.FrankenMarkdown.Widgets`
- Share extension: `com.frankenmarkdown.FrankenMarkdown.Share`
- App Group: `group.com.frankenmarkdown.FrankenMarkdown`
- URL scheme: `frankenmarkdown://`
- Deployment: iOS/iPadOS 17+, Mac Catalyst 14+ through the iOS 17 deployment target
- Device families: iPhone and iPad; Mac Catalyst uses native design
- Initial version: `1.0`, build `1`
- Category: Productivity
- Price: Free, worldwide where App Store Connect permits

## 12. Verification gates

1. Run `scripts/check-wasm-package.sh <run-id>` and record native/WASM byte parity plus size budget.
2. Run `ios/sync-renderer.sh --check` and verify the recorded SHA-256 manifest.
3. Generate the project with XcodeGen and ensure regeneration produces no unreviewed source-of-truth drift.
4. Unit-test scheme-path canonicalization, MIME mapping, bridge schema validation, request cancellation, chunk order, size limits, and diagnostic span conversion.
5. UI-test new/open/edit/undo/import/preview/export/share flows, including empty, malformed, large, raw-HTML, custom-font, and attachment documents.
6. Build Debug and Release for Apple Silicon iPhone simulator, iPad simulator, physical iPhone, physical iPad, and arm64 Mac Catalyst.
7. Test Dynamic Type XXXL, VoiceOver, Reduce Motion, Reduce Transparency, increased contrast, light/dark appearance, Stage Manager widths, hardware keyboard, pointer, and memory pressure.
8. Confirm the app launches and renders with all network interfaces disabled.
9. Archive and validate with the distribution profile; inspect entitlements, privacy manifest, bundle contents, and absence of unexpected URLs/SDKs.
10. Capture real iPhone, iPad, and Mac screenshots only from the final signed release candidate.

## 13. Delivery order

1. Freeze and verify the renderer package.
2. Add plan-reviewed XcodeGen skeleton, privacy files, identifiers, and resources.
3. Implement the resource scheme and versioned bridge with tests.
4. Implement document model/editor/import/state restoration.
5. Implement live preview, diagnostics, export, Quick Look, and sharing.
6. Implement adaptive iPhone/iPad/Mac compositions and commands.
7. Implement the document reactor, accessibility reductions, and suite polish.
8. Add Share extension, widget, App Intents, Spotlight opt-in, and Handoff.
9. Install and test on real devices and Mac, then fresh-eyes review all new code.
10. Create the App Store Connect record only after bundle identity, signing, icon, privacy posture, and builds are real.
