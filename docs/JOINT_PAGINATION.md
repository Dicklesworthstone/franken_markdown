# Joint paragraph-variant and page planning

`franken_markdown::pagination::plan_pagination` plans a complete, uniform-line-grid
publication from `layout::ParagraphCandidates`. It jointly chooses a measured
line-breaking variant for every paragraph and page boundaries, including breaks
inside long paragraphs. It returns the chosen variants and every half-open line
fragment, not merely the paragraphs at which a page starts.

This is an additive core API. It does **not** replace the existing PDF renderer,
`PdfOptions::optimal_pagination`, or the legacy
`layout::solve_2d_optimal_pagination` heuristic. Mixed-height PDF blocks,
footnote reservations, table-header repetition and PDF object emission remain
owned by that renderer. A line-grid plan is not evidence of improved PDF output.

## Use the selected variants and fragments together

```rust
use franken_markdown::pagination::{
    plan_pagination, PaginationOptions, ParagraphPolicy,
};

// candidates: &[franken_markdown::layout::ParagraphCandidates]
// Each candidate owns the actual LineBreak values produced by the host's
// line-breaking phase. Count-only candidates may leave `lines` empty.
let policies = vec![ParagraphPolicy::default(); candidates.len()];
let plan = plan_pagination(
    candidates,
    &policies,
    PaginationOptions {
        page_capacity_lines: 48,
        max_pages: Some(12),
        ..PaginationOptions::default()
    },
)?;
for fragment in &plan.fragments {
    let chosen = &candidates[fragment.paragraph_index]
        .variants[fragment.variant_chosen];
    // With measured (not count-only) candidates:
    let lines = &chosen.lines[fragment.line_start..fragment.line_end];
    // Place these lines on fragment.page_index, starting at
    // fragment.page_line_start. Do not reflow a different variant afterward.
}
```

All coordinates are zero-based. `initial_used_lines` accounts for content already
on the first page without inventing a fragment for it. Input candidates are
borrowed and unchanged. Empty input yields no pages unless the first page is
already occupied. Empty variant sets, zero-line variants, and inconsistent
nonempty line arrays are rejected instead of silently losing paragraphs.

## Constraints and objective

By default, internal paragraph breaks must leave at least two lines before the
break (`orphans`) and at least two lines in the continuation fragment (`widows`).
A middle fragment is subject to both applicable minima. One-line paragraphs that
fit without an internal break remain legal. Minimum zero disables that minimum.
A `None` penalty makes a minimum hard; `Some(weight)` explicitly permits each
missing line at the supplied cost. The planner never silently relaxes a hard rule.

`ParagraphPolicy` can require a paragraph to stay together, bind its last fragment
to the following paragraph's first fragment, or force a page before it. A forced
break conflicting with a keep bond is infeasible. No empty pages are introduced,
including for a forced break at the start of an empty publication.

The exact signed objective is:

```
sum(chosen variant demerits)
+ page_cost * nonempty page count
+ unused_line_cost * sum(unused lines squared)
+ explicitly permitted widow/orphan deficits
```

The last page has no unused-line penalty unless `penalize_last_page` is enabled.
Costs use checked `i128` arithmetic; negative `i64` line-breaking demerits retain
their meaning. Equal scores prefer fewer pages, followed by stable traversal
order. No clock, floating-point cost, hash iteration, I/O, or new dependency is
involved.

`max_pages` is an optional **hard** positive bound, counting an occupied initial
page. It is not a cost that can be traded away. With this bound, the search retains
states separately by page count as well as occupancy. Otherwise, a cheap path
using more pages could incorrectly discard the only path that can finish within
the limit. Continuation states carry the same additional dimension.

For example, with capacity 5, paragraph A may have a 10-line variant costing 0 or
a 5-line variant costing 200; paragraph B has 5 lines. At page cost 100, the
unconstrained optimum uses the 10-line variant and three pages (cost 300).
With `max_pages: Some(2)`, the correct answer uses the 5-line variant and two pages
(cost 400). Occupancy-only state pruning cannot safely solve this constrained case.

## Search and failure contract

The search is an acyclic shortest-path dynamic program. At a paragraph boundary,
future choices depend on occupied lines, fixed paragraph policies, and used page
count when bounded. Inside one selected variant, continuation choices depend on
the next unplaced line and, when bounded, used page count. Dominance compares only
states with identical future constraints. Backpointers reconstruct a complete
ordered line partition without recursively copying candidate paths.

Explicit `PaginationLimits` bound paragraphs, variants, candidate lines, page
capacity, transitions, and retained backpointer nodes. Exhaustion, arithmetic
overflow or infeasibility returns a typed error, never a partial result advertised
as optimal. Raising a work budget can permit a larger exact search; it does not
change the objective or silently switch to a heuristic.

`src/pagination_tests.rs` contains coverage/cost/constraint audits and an
independent unpruned exhaustive oracle. Run the actual Rust tests with
`cargo test pagination` in the repository's configured toolchain. An independent
executable model was also compared against enumeration during implementation;
that model check is not a Rust compilation, WASM, PDF visual, or benchmark result.
