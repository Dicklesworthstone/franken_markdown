//! Exact joint paragraph-variant selection and line-grid pagination.
//!
//! The caller supplies measured paragraph candidates from the line breaker.
//! This is a bounded shortest-path search, not a greedy best-fit pass: a more
//! expensive short paragraph can avoid an expensive page later in the document.
//! Every successful plan contains the selected variant AND every line fragment.
//! All lines are placed once, in order, inside the declared page capacity.
//!
//! The objective is the sum of chosen variant demerits, a cost per nonempty page,
//! squared unused lines on closed pages, and explicitly enabled constraint
//! violations. The last page is ragged for free by default. Equal-cost paths
//! prefer fewer pages, then stable variant/occupancy/line traversal order.
//!
//! This module models a uniform baseline grid. It does not measure paragraphs,
//! reserve footnote areas, or emit PDF. The existing PDF renderer is unchanged;
//! hosts must use the returned variants and fragments together, not just breaks.

use crate::layout::ParagraphCandidates;
use std::collections::BTreeMap;
use std::fmt;

/// Structural constraints for one paragraph. An empty policy slice means all
/// defaults. `keep_with_next` binds its LAST fragment to the next paragraph's
/// FIRST fragment; it does not prevent splitting a long paragraph internally.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ParagraphPolicy {
    pub keep_together: bool,
    pub keep_with_next: bool,
    pub break_before: bool,
}

/// Admission/work limits. Exhaustion returns an error, never a supposedly
/// optimal partial answer. These bound search work, not a wall-clock deadline.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PaginationLimits {
    pub max_paragraphs: usize,
    pub max_variants_per_paragraph: usize,
    pub max_candidate_lines: usize,
    pub max_page_capacity: usize,
    pub max_nodes: usize,
    pub max_transitions: usize,
}

impl Default for PaginationLimits {
    fn default() -> Self {
        Self {
            max_paragraphs: 10_000,
            max_variants_per_paragraph: 32,
            max_candidate_lines: 1_000_000,
            max_page_capacity: 4096,
            max_nodes: 1_000_000,
            max_transitions: 8_000_000,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PaginationOptions {
    pub page_capacity_lines: usize,
    /// Already occupied lines on the first page. No synthetic fragment is
    /// returned for this content, but its page and unused space are costed.
    pub initial_used_lines: usize,
    /// Minimum lines in a fragment before an internal paragraph break.
    pub orphans: usize,
    /// Minimum lines in a fragment continuing after an internal break.
    pub widows: usize,
    /// None makes the minimum a hard constraint. Some(cost) permits a violation
    /// at that cost PER missing line, including zero for an explicit relaxation.
    pub orphan_penalty: Option<u64>,
    pub widow_penalty: Option<u64>,
    pub page_cost: u64,
    pub unused_line_cost: u64,
    pub penalize_last_page: bool,
    pub limits: PaginationLimits,
}

impl Default for PaginationOptions {
    fn default() -> Self {
        Self {
            page_capacity_lines: 48,
            initial_used_lines: 0,
            orphans: 2,
            widows: 2,
            orphan_penalty: None,
            widow_penalty: None,
            page_cost: 10_000,
            unused_line_cost: 1,
            penalize_last_page: false,
            limits: PaginationLimits::default(),
        }
    }
}

/// A half-open line range in ONE chosen paragraph variant. Coordinates are
/// zero-based; `page_line_start` includes occupied lines on the initial page.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PageFragment {
    pub paragraph_index: usize,
    pub variant_chosen: usize,
    pub line_start: usize,
    pub line_end: usize,
    pub page_index: usize,
    pub page_line_start: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PaginationPlan {
    /// One index for every input paragraph, including ones not at page breaks.
    pub variants: Vec<usize>,
    pub fragments: Vec<PageFragment>,
    pub page_count: usize,
    /// Exact signed sum; negative line-breaking demerits are preserved.
    pub total_demerits: i128,
    pub search_nodes: usize,
    pub search_transitions: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PaginationError {
    InvalidOptions(&'static str),
    InvalidParagraph { paragraph_index: usize, reason: &'static str },
    BudgetExceeded(&'static str),
    CostOverflow,
    /// Includes hard widow/orphan, keep, and forced-break conflicts.
    NoFeasibleLayout { paragraph_index: usize },
}

impl fmt::Display for PaginationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidOptions(reason) => write!(f, "invalid pagination options: {reason}"),
            Self::InvalidParagraph { paragraph_index, reason } => {
                write!(f, "invalid paragraph {paragraph_index}: {reason}")
            }
            Self::BudgetExceeded(budget) => write!(f, "pagination budget exceeded: {budget}"),
            Self::CostOverflow => f.write_str("pagination cost or page counter overflow"),
            Self::NoFeasibleLayout { paragraph_index } => {
                write!(f, "no feasible pagination through paragraph {paragraph_index}")
            }
        }
    }
}
impl std::error::Error for PaginationError {}

#[derive(Clone, Copy, Debug)]
struct State {
    cost: i128,
    closed_pages: usize,
    used: usize,
    tail: Option<usize>,
}
impl State {
    fn rank(self) -> (i128, usize) { (self.cost, self.closed_pages) }
}

struct Node {
    previous: Option<usize>,
    fragment: PageFragment,
}
struct Search {
    options: PaginationOptions,
    nodes: Vec<Node>,
    transitions: usize,
}

impl Search {
    fn charge(&mut self) -> Result<(), PaginationError> {
        if self.transitions >= self.options.limits.max_transitions {
            return Err(PaginationError::BudgetExceeded("transitions"));
        }
        self.transitions += 1;
        Ok(())
    }

    fn close_page(&self, mut state: State, last: bool) -> Result<State, PaginationError> {
        if state.used == 0 { return Ok(state); }
        let mut cost = i128::from(self.options.page_cost);
        if !last || self.options.penalize_last_page {
            let gap = i128::try_from(self.options.page_capacity_lines - state.used)
                .map_err(|_| PaginationError::CostOverflow)?;
            let ragged = gap.checked_mul(gap)
                .and_then(|n| n.checked_mul(i128::from(self.options.unused_line_cost)))
                .ok_or(PaginationError::CostOverflow)?;
            cost = cost.checked_add(ragged).ok_or(PaginationError::CostOverflow)?;
        }
        state.cost = state.cost.checked_add(cost).ok_or(PaginationError::CostOverflow)?;
        state.closed_pages = state.closed_pages.checked_add(1).ok_or(PaginationError::CostOverflow)?;
        state.used = 0;
        Ok(state)
    }

    // Only improving states receive backpointers. Discarded nodes are retained
    // because later tails can still refer to them; max_nodes bounds the arena.
    fn retain(
        &mut self, frontier: &mut BTreeMap<usize, State>, key: usize,
        mut candidate: State, fragment: PageFragment,
    ) -> Result<(), PaginationError> {
        if frontier.get(&key).is_some_and(|old| old.rank() <= candidate.rank()) {
            return Ok(());
        }
        if self.nodes.len() >= self.options.limits.max_nodes {
            return Err(PaginationError::BudgetExceeded("backpointer nodes"));
        }
        let index = self.nodes.len();
        self.nodes.push(Node { previous: candidate.tail, fragment });
        candidate.tail = Some(index);
        frontier.insert(key, candidate);
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    fn place(
        &mut self, state: State, paragraph: usize, variant: usize, start: usize,
        lines: usize, policy: ParagraphPolicy,
        completed: &mut BTreeMap<usize, State>, pending: &mut BTreeMap<usize, State>,
    ) -> Result<(), PaginationError> {
        let capacity = self.options.page_capacity_lines - state.used;
        let remaining = lines - start;
        let available = remaining.min(capacity);
        // Longest fragments first make deterministic ties prefer filling pages.
        // Complete and split placements are BOTH considered when both are legal.
        let minimum = if policy.keep_together { remaining } else { 1 };
        for count in (minimum..=available).rev() {
            self.charge()?;
            let end = start + count; // bounded by admitted line count
            let split = end < lines;
            let mut penalty = 0_i128;
            if split {
                let Some(cost) = violation(count, self.options.orphans, self.options.orphan_penalty)? else { continue; };
                penalty = cost;
            }
            if start > 0 {
                let Some(cost) = violation(count, self.options.widows, self.options.widow_penalty)? else { continue; };
                penalty = penalty.checked_add(cost).ok_or(PaginationError::CostOverflow)?;
            }
            let mut candidate = State {
                cost: state.cost.checked_add(penalty).ok_or(PaginationError::CostOverflow)?,
                used: state.used + count, ..state
            };
            let fragment = PageFragment {
                paragraph_index: paragraph, variant_chosen: variant,
                line_start: start, line_end: end, page_index: state.closed_pages,
                page_line_start: state.used,
            };
            if split {
                candidate = self.close_page(candidate, false)?;
                self.retain(pending, end, candidate, fragment)?;
            } else {
                self.retain(completed, candidate.used, candidate, fragment)?;
            }
        }
        Ok(())
    }
}

fn violation(count: usize, minimum: usize, penalty: Option<u64>) -> Result<Option<i128>, PaginationError> {
    let missing = minimum.saturating_sub(count);
    if missing == 0 { return Ok(Some(0)); }
    let Some(penalty) = penalty else { return Ok(None); };
    let cost = i128::try_from(missing).map_err(|_| PaginationError::CostOverflow)?
        .checked_mul(i128::from(penalty)).ok_or(PaginationError::CostOverflow)?;
    Ok(Some(cost))
}

fn validate(
    paragraphs: &[ParagraphCandidates], policies: &[ParagraphPolicy], options: PaginationOptions,
) -> Result<(), PaginationError> {
    if options.page_capacity_lines == 0 || options.initial_used_lines > options.page_capacity_lines {
        return Err(PaginationError::InvalidOptions("capacity must be positive and contain the initial occupied lines"));
    }
    if !policies.is_empty() && policies.len() != paragraphs.len() {
        return Err(PaginationError::InvalidOptions("policies must be empty or have one entry per paragraph"));
    }
    if paragraphs.len() > options.limits.max_paragraphs {
        return Err(PaginationError::BudgetExceeded("paragraphs"));
    }
    if options.page_capacity_lines > options.limits.max_page_capacity {
        return Err(PaginationError::BudgetExceeded("page capacity"));
    }
    let mut total = 0usize;
    for (paragraph_index, paragraph) in paragraphs.iter().enumerate() {
        if paragraph.variants.is_empty() {
            return Err(PaginationError::InvalidParagraph { paragraph_index, reason: "no line-breaking variants" });
        }
        if paragraph.variants.len() > options.limits.max_variants_per_paragraph {
            return Err(PaginationError::BudgetExceeded("variants per paragraph"));
        }
        for variant in &paragraph.variants {
            if variant.line_count == 0 {
                return Err(PaginationError::InvalidParagraph { paragraph_index, reason: "zero-line variant" });
            }
            if !variant.lines.is_empty() && variant.lines.len() != variant.line_count {
                return Err(PaginationError::InvalidParagraph { paragraph_index, reason: "line count does not match measured lines" });
            }
            total = total.checked_add(variant.line_count)
                .filter(|&n| n <= options.limits.max_candidate_lines)
                .ok_or(PaginationError::BudgetExceeded("candidate lines"))?;
        }
    }
    Ok(())
}

/// Find the exact minimum-cost plan under the declared line-grid model.
///
/// At paragraph boundaries, future costs depend only on occupied lines (and
/// fixed adjacent policies), so only the best state per occupancy is retained.
/// Within one selected variant, continuation states depend only on the next
/// unplaced line and start on an empty page. Both are acyclic shortest paths;
/// dominance never discards a state with a different future constraint context.
///
/// Source candidates are borrowed, never mutated or cloned. Empty `lines` arrays
/// permit count-only planning; nonempty arrays must match the stated line count.
/// Empty input is a zero-page plan unless the initial page is already occupied.
/// Hard constraints are never silently relaxed. No partial plan is returned on
/// invalid inputs, infeasibility, cost overflow, or exhausted search budgets.
///
/// # Errors
/// Returns [`PaginationError`] when admission, feasibility, checked arithmetic,
/// or the configured search budget prevents a complete exact plan.
pub fn plan_pagination(
    paragraphs: &[ParagraphCandidates], policies: &[ParagraphPolicy], options: PaginationOptions,
) -> Result<PaginationPlan, PaginationError> {
    validate(paragraphs, policies, options)?;
    let policy_at = |index| policies.get(index).copied().unwrap_or_default();
    let mut search = Search { options, nodes: Vec::new(), transitions: 0 };
    let initial = State { cost: 0, closed_pages: 0, used: options.initial_used_lines, tail: None };
    let mut frontier = BTreeMap::from([(initial.used, initial)]);
    for (paragraph_index, paragraph) in paragraphs.iter().enumerate() {
        let policy = policy_at(paragraph_index);
        let keep_previous = paragraph_index > 0 && policy_at(paragraph_index - 1).keep_with_next;
        let mut completed = BTreeMap::new();
        for (variant_index, variant) in paragraph.variants.iter().enumerate() {
            let mut pending = BTreeMap::new();
            for &previous in frontier.values() {
                search.charge()?;
                let state = State {
                    cost: previous.cost.checked_add(i128::from(variant.demerits))
                        .ok_or(PaginationError::CostOverflow)?, ..previous
                };
                if !policy.break_before || state.used == 0 {
                    search.place(state, paragraph_index, variant_index, 0, variant.line_count,
                        policy, &mut completed, &mut pending)?;
                }
                // Opening a page is optional, not only an overflow response.
                // Never manufacture empty pages or break a keep-with-next bond.
                if state.used > 0 && !keep_previous {
                    let state = search.close_page(state, false)?;
                    search.place(state, paragraph_index, variant_index, 0, variant.line_count,
                        policy, &mut completed, &mut pending)?;
                }
            }
            while let Some((start, state)) = pending.pop_first() {
                search.place(state, paragraph_index, variant_index, start, variant.line_count,
                    policy, &mut completed, &mut pending)?;
            }
        }
        if completed.is_empty() {
            return Err(PaginationError::NoFeasibleLayout { paragraph_index });
        }
        frontier = completed;
    }
    let mut best: Option<State> = None;
    for state in frontier.into_values() {
        let candidate = search.close_page(state, true)?;
        if best.is_none_or(|old| candidate.rank() < old.rank()) {
            best = Some(candidate);
        }
    }
    let best = best.ok_or(PaginationError::NoFeasibleLayout { paragraph_index: 0 })?;
    let mut fragments = Vec::new();
    let mut cursor = best.tail;
    while let Some(index) = cursor {
        let node = &search.nodes[index];
        fragments.push(node.fragment);
        cursor = node.previous;
    }
    fragments.reverse();
    let mut variants = vec![0; paragraphs.len()];
    for fragment in &fragments { variants[fragment.paragraph_index] = fragment.variant_chosen; }
    Ok(PaginationPlan {
        variants, fragments, page_count: best.closed_pages, total_demerits: best.cost,
        search_nodes: search.nodes.len(), search_transitions: search.transitions,
    })
}

#[cfg(test)]
#[path = "pagination_tests.rs"]
mod tests;
