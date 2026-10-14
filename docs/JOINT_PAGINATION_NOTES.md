# Joint paragraph selection with page-bottom notes

`PdfOptions::optimal_pagination` now admits same-measure paragraph alternatives
in documents prepared by `footnotes::for_pdf_paged`. Previously, any footnote
definition disabled candidate collection, and the selection stage separately
refused a page-note region. Both barriers prevented otherwise useful paragraph
choices throughout a cited report or book.

## Reservation and selection contract

The baseline variant uses the existing `PageNotes::line_reserve` calculation.
An alternative must preserve the exact ordered stream of numbered citation
links, including repetitions. Only note bodies first cited in that logical block
are re-anchored to the alternative's physical lines. Earlier references do not
charge the same note again. Missing, additional, or reordered citations reject
that alternative without modifying the baseline.

Measured page-note bodies and the separator stay in the trailing note region;
they are not ordinary body blocks in the joint search. The original heights,
text, links, styles, and structure are retained. After selecting paragraph shapes,
the bridge scans the complete replacement layout again and asks the same
footnote-reserving planner used by the emitter to reproduce the proposed page
boundaries. Failure or budget exhaustion leaves the original layout untouched.
The existing TOC convergence, fitting, verification, and emitters consume that
single selected layout.

The reservation policy remains deliberately conservative: one separator is
charged per physical line introducing notes, even when multiple such lines land
on the same page. Multiple first citations on one line share its separator.
The actual emitter still prints only one separator per page. This change does
not introduce note-body splitting or a new per-page note-reservation objective.
Existing oversized/unreferenced note fallback remains in the note-layout stage.

## Scope and validation

The default nonoptimal path and the gradual, river, and Pareto typography modes
are unchanged. Unprepared definition IDs retain the original baseline policy.
The existing eligibility restrictions, hard split policies, search budgets, and
no-scaling paragraph alternatives remain in force.

Twelve native regressions in `src/pdf/joint_pagination_note_tests.rs` cover
admission, exact baseline reservation parity, moved and repeated references,
shared separators, changed citation streams, actual measured page savings,
retained note-region geometry, citation-page placement, deterministic PDF
emission, verification, nonoptimal behavior, and infeasible foot space.
Run with the project's prescribed remote compilation route:

```sh
rch exec -- cargo test --lib pdf::joint_pagination
```

Implementation-session validation (2026-10-14): an independent exhaustive model
checked 45,214 block-local re-anchoring cases against a complete first-reference
rescan, including float32 accumulation order. This is an algorithm check, not
execution of the Rust code. Cargo, rustc, rustfmt, rch, and dsr were unavailable;
the native tests, formatting gate, complete build, and visual/native-WASM parity
checks have not been run. No native-rendering or passing-native-test claim is
made by this change.
