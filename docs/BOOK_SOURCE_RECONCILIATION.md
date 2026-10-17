# Reconcile editor and external source edits

The book workbench's **Connected source folder** panel now exposes
**Reconcile external edits**. It uses the existing source-folder connection and
its reviewed paths. It does not enumerate a directory, write files, delete
sources, restore resource permissions, or capture publishing settings.

The existing **Review external source changes** action keeps its conservative
whole-file refresh behavior. The new action enables the existing three-way
source reconciliation and adds region-level review for overlapping edits.

## Review before applying

Connect the selected source folder first. After editing either the book or its
disk sources, choose Reconcile external edits. Separate line edits are combined;
overlapping edits require an explicit decision. The review lists every source
and its action. Nothing is installed until **Apply reviewed reconciliation**.

For each conflicting file, choose the complete editor version, the complete disk
version (when present), a manually edited complete replacement, or individual
conflicting regions. Whole-file choices intentionally discard the other
version's changes. Region choices keep **all non-conflicting edits from both
sides** and select editor or disk text only for each overlapping region. Every
region must be selected; there is no default winner and no implicit concatenation
of competing insertions. Manual complete replacements may be empty.

Base/editor/disk excerpts are inert text, limited to 1,024 Unicode scalars each.
They are labeled as excerpts, not a complete diff. The resolution operates on the
complete retained strings, never those display excerpts. An untouched manual
textarea preserves the imported source's BOM and newline convention.

The complete source set is applied through the host's existing synchronous
source-only transaction. Chapter order, include-only roles, settings and asset
grants remain current. The disk session rechecks the complete reviewed file
identities and bytes and the live editor checkpoint before applying anything.
Typing in the source editor, changing the project, starting an import, cancelling,
suspending, or disposing cannot allow an old decision to overwrite newer input.
Text composition inside a manual resolution temporarily blocks approval.

Reconciliation itself performs **no disk writes**. Retained local or merged
changes still require a separate, explicitly reviewed Save source files action.
An absent disk file is not an empty source or permission to delete a chapter.
Cancellation leaves both sources and the old comparison baseline untouched.

## Sparse changes and bounded work

The previous merger allocated a longest-common-subsequence matrix between the
first and last changed lines. Several tiny, distant edits in one long chapter
could therefore exhaust its allowance. The merger now retains that algorithm
when its matrix fits, but otherwise explores shortest edit paths using bounded
frontiers. A 10,000-line regression with four distant edits now produces the
exact combined source instead of a `merge-budget` conflict.

The same two-million-unit allowance is shared across all files in a session
review. Matrix entries, retained frontier entries and frontier line comparisons
are charged before allocation/work. The existing 100,000-line aggregate and
4,194,304 UTF-16-code-unit input/result limits remain; the disk session separately enforces UTF-8
file/session limits. Dense divergence and exhausted allowances still require
explicit whole-file resolution. No larger budget or guessed merge is substituted.
These are logical work/content bounds, not wall-time or browser-heap guarantees.

The region review adds one separately bounded planning pass over conflict files,
sharing the same default allowance across that pass. It exposes at most 256
regions in total. Budget or region-limit refusals retain whole-file choices and
manual replacement; they never present a truncated region list as complete.

## Programmatic region planning

`createBookSourceMerger(limits)` keeps its existing callable interface and
three-field result shape. Its additive `.plan(base, local, disk)` method uses
the same diff and shares that merger's remaining allowance. An overlapping
result can additionally expose an immutable `conflicts` array and `resolve`:

```js
const merger = createBookSourceMerger();
const plan = merger.plan(baseSource, editorSource, diskSource);
if (plan.conflicts) {
  const source = plan.resolve(plan.conflicts.map(hunk => ({
    index: hunk.index,
    choice: "local", // Explicitly choose local, disk, or edited per region.
  })));
}
```

Each region carries `index`, zero-based `startLine`, exclusive `endLine`, and
complete `base`, `local`, and `disk` text. An insertion has an empty base range.
Transitive overlap groups include every intersecting change, including deletions
spanning several opposing edits. Identical changes are emitted once. An edited
region uses `{index, choice: "edited", source: "replacement"}`. Duplicate,
missing, accessor-backed, out-of-range or malformed choices are rejected, as are
invalid manual Unicode and oversized combined output. Resolving does not mutate
or consume the plan. Creation/deletion/budget refusals have no invented regions.

This is literal source reconciliation, not Markdown-aware refactoring or proof
that the merged manuscript has valid links. Review and verify the resulting book
before publishing it.

## Validation

```sh
node --test wasm/tests/book_directory_merge.test.mjs \
  wasm/tests/book_directory_merge_sparse.test.mjs \
  wasm/tests/book_directory_merge_hunks.test.mjs \
  wasm/tests/book_directory_reconcile_controls.test.mjs
```

All 54 focused Node tests pass: 10 existing unchanged merge cases, 11 sparse
regressions, 15 region-planning cases and 18 workbench/session cases. The seeded
sparse test covers 500 insert/delete/replace trials in both directions against
an independent source oracle. The old production implementation was run against
the long-source witness and returned `merge-budget`.

Integration imports the complete production controller, merger and unchanged
directory session. DOM, host source transactions and browser file handles are
explicit doubles; encoding uses native Node File/Blob facilities. This is not
real-browser IME/focus/accessibility acceptance, OS filesystem race qualification,
a Rust/WASM build, or a full-repository test pass. No renderer or build policy is
changed and no GitHub Actions workflow is used.
