# Joint paragraph-variant and page planning

## PDF rendering

`fmd input.md --to pdf --pdf-optimal-pagination` (or
`PdfOptions::optimal_pagination = true`) now considers alternative paragraph
shapes as well as page boundaries. The normal paragraph layout remains one
choice. The existing Knuth–Plass candidate generator can offer layouts with one
fewer or one additional line **at the same text width and font size**. Each
alternative must satisfy its ordinary line-fit constraints; page packing does
not license an overfull line or scale down the text.

The mixed-height planner, `pagination::height::plan_blocks`, combines the actual
line-breaking demerits with page costs and existing pagination constraints.
Shorter paragraphs can therefore avoid an extra page, while an alternative with
poor word spacing can cost more than the page it would save. There is no fixed
preference for the shortest shape.

Selection happens after block layout has finalized paragraph spacing and forced
chapter breaks. The selected positioned text segments become the input to PDF
emission, TOC page-number convergence, page-budget fitting, and text verification.
Links and inline styles stay attached to their original text. A substitution is
accepted only when the existing page planner reproduces the selected page
boundaries from those final lines. Both streaming and monolithic PDF emission use
that same result.

### Current admission limits

The first integration offers alternatives for ordinary body paragraphs only,
with at least four physical lines in both the baseline and every offered shape.
This preserves the existing context-sensitive rules for short captions, heading
followers, and final list items. Other blocks still participate in page planning
at their existing measured shape.

Paragraphs inside lists or blockquotes, paragraphs containing inline images or
typeset math, and documents containing footnote definitions retain their baseline
paragraph shapes. The same applies when gradual-demerit, river, or Pareto line
breaking is enabled: their additional predecessor state is not represented by
the adjacent-line-count candidate search. These cases continue to use the
existing optimal page planner, including its footnote reservations and repeated
table headers.

Candidate generation is capped at 1,024 paragraph items and 16,384 retained
alternative lines per layout pass, in addition to the candidate generator's
state-cell bound and the page planner's existing work budgets. Each attempted
search also consumes a document-wide work allowance: the conservative estimate
`items² × (baseline lines + 2)` is charged before searching, with a total cap of
67,108,864 per layout pass. Attempts that produce no usable alternatives still
consume that allowance. Reaching a bound keeps the original measured paragraph.
An infeasible or exhausted joint search keeps the complete original layout and
its existing pagination fallback.
Default PDF rendering, without `optimal_pagination`, does not collect or select
alternative paragraph shapes.

### A measured PDF regression

The fixed prose fixture in `src/pdf/joint_pagination_tests.rs` uses the bundled
fonts at the normal 11 pt body size, a 440 pt content width, and a 196 pt content
height (a 512 × 268 pt page with 36 pt margins). Its ordinary measured paragraph
has 14 lines and needs two pages when the planner respects the paragraph's final
gap and two-line widow/orphan minima. A feasible 13-line alternative fits on one
page, with the same width and font size. Its extra line-breaking cost is 13,455,
less than the planner's existing 50,000 cost for an additional page.

This is a specific example, not a promise that every document becomes shorter.
A second fixture offers a much more expensive shorter shape and retains its
original paragraph. The tests also exercise links and tagged text after an
actual substitution, unchanged font size when fitting to one page, forced
chapter boundaries, repeated paragraphs, and TOC numbers matching emitted
heading destinations. Default and unsupported-mode cases compare emitted bytes
with the original measured layout. Run these regressions with
`cargo test --lib joint_pagination`.

## Uniform-grid core API

`franken_markdown::pagination::plan_pagination` plans a complete, uniform-line-grid
publication from `layout::ParagraphCandidates`. It jointly chooses a measured
line-breaking variant for every paragraph and page boundaries, including breaks
inside long paragraphs. It returns the chosen variants and every half-open line
fragment, not merely the paragraphs at which a page starts.

This additive line-grid API is separate from the mixed-height planner used by
PDF rendering and from the legacy `layout::solve_2d_optimal_pagination` heuristic.
A line-grid plan alone is not evidence of improved PDF output.

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
