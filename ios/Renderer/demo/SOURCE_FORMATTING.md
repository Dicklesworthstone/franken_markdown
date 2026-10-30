# Markdown source authoring

The live editor's independent `flow-source.js` entrypoint installs twelve
formatting buttons: Bold, Italic, Strike, Inline code, Code block, Heading 1–3,
Bullets, Numbered list, Task list and Quote. Source-scoped Ctrl/Cmd+B,
Ctrl/Cmd+I and Ctrl/Cmd+Shift+X invoke the emphasis commands. Native buttons
support keyboard activation; no page-wide key handler or Tab interception is
installed. The commands do not require a renderer, worker, fonts or storage.

## Source and selection semantics

`planSourceCommand(source, selection, command)` returns one immutable replacement
in original UTF-16 coordinates and the resulting selection. It never changes its
input or works backward from rendered text. Source before and after the affected
range is untouched. Source and results are bounded to 4 MiB UTF-8; line commands
admit at most 10000 selected lines. Malformed Unicode, scalar/surrogate interiors,
CRLF interiors and invalid selection coordinates are rejected before publication.

Emphasis commands toggle literal selected or immediately surrounding delimiters.
Nested strong/emphasis uses three stars without stripping half a strong delimiter.
Leading/trailing selection whitespace stays outside new emphasis. An empty caret
inserts a selected `text` placeholder. Inline commands require a single line.
This is a source operation, not inferred active-format state: ambiguous Markdown
nesting, escaped markers and surrounding syntax are not parsed or normalized.

Inline code inserts a backtick delimiter longer than every run in the selected
text. Padding preserves literal boundary backticks and spaces. Code block wraps
complete selected lines with a similarly collision-free fence. Code commands
insert wrappers rather than toggling existing code syntax; Undo removes them.
An empty inline-code selection receives a selected `code` placeholder.

Heading/list/quote commands transform complete affected lines; a selection ending
at the next line's start excludes that line. Blank selected lines stay blank,
indentation and mixed CR/LF separators are preserved by the planner, and ordered
lists number nonblank lines from 1. Existing prefixes in the requested family
are replaced; applying a uniform prefix again removes it. Task-list conversion
starts unchecked; removing task markers removes their checked state explicitly.
These operations do not infer nested list ownership or treat code as a separate
editing language. The textarea itself retains its existing LF normalization.

## One ordinary undoable edit

The controller focuses the textarea, then rechecks the original source,
selection and document lifecycle. It emits cancelable `beforeinput`, rechecks
again, performs one `setRangeText`, restores selection and emits `input`. The
existing source-history controller captures that as one transaction. Preview,
draft scheduling and prepared-download invalidation observe the ordinary input;
no second history or persistence mechanism is introduced.

Composition, readonly/disabled state, repeated shortcut events, disposal and
reentrant commands are refused. Focus/beforeinput handlers that replace source,
move the selection or signal document replacement revoke the pending plan. A
host may cancel beforeinput without a phantom history entry. Once observers have
seen an accepted input, the controller never claims their side effects rolled
back. Page suspension removes the toolbar and listeners; persisted restoration
installs one fresh owner alongside the existing fresh history.

## Verification

```sh
node --test wasm/tests/source_commands*.test.mjs
node --test wasm/tests/flow_source_entry.test.mjs
python3 wasm/tests/run_source_formatting.py --chromium /usr/bin/chromium
```

The 35 planner tests executed successfully, including 512 generated fixtures
round-tripped through five line commands. The 23 Chromium checks exercise the
actual new entrypoint/controller/planner with native buttons, textarea selection,
keyboard input and focus. The runner extracts existing editing/history and
source-validator declarations from their modules; file I/O, drafts and direct
file controls are explicit doubles. Module imports are linked to local Blob
URLs; network requests are blocked. The authoring run used locally materialized
upstream source-control declarations, not a complete cloned checkout.

The existing Node entrypoint fixture was extended to support the new DOM and a
formatting/download/undo case without removing its prior assertions. That full
entrypoint suite, DSR/native builds, package assembly and generated WASM were not
executed here. The manifest and both assembly scripts now include the new runtime
modules and this document; existing build/parity/size gates are unchanged.
This work does not change the still-pending PDF ragged-policy handoff.
