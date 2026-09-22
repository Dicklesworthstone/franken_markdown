//! Exact mixed-height block pagination.
//!
//! This is the variable-height counterpart to the line-grid planner in the
//! parent module. A block exposes one or more measured variants. Each variant
//! is an ordered sequence of indivisible fragments with deterministic heights:
//! paragraph lines, table rows, code lines, image boxes, or any other vertical
//! unit already measured by the caller. The planner jointly chooses a variant,
//! page boundaries, and internal block split points.
//!
//! Heights use the renderer's fixed milli-point `LayoutUnit`. The search never
//! remeasures content and never uses floating point. Hard keep/forced-break
//! constraints are never silently relaxed. Work and backpointer budgets make
//! failure explicit instead of returning a partial plan advertised as optimal.

use crate::layout::{LayoutUnit, ParagraphCandidates};
use std::collections::BTreeMap;
use std::fmt;

/// One measured layout alternative for a block.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BlockVariant {
    /// Line/layout quality cost supplied by the upstream layout engine.
    pub demerits: i64,
    /// Positive heights of indivisible fragments, in reading order.
    pub fragment_heights: Vec<LayoutUnit>,
    /// Extra vertical material repeated before every continuation fragment
    /// after an internal page break, e.g. a repeated table header.
    pub continuation_prefix: LayoutUnit,
    /// Bottom-of-page reservations introduced by each fragment, e.g. footnote
    /// bodies anchored by text in that fragment. Empty means all zero; otherwise
    /// this must have exactly one entry per fragment.
    pub fragment_reservations: Vec<LayoutUnit>,
    /// Policy for each internal boundary after fragment i. Empty means every
    /// boundary is allowed at zero cost. Otherwise length must be
    /// fragment_heights.len() - 1; None forbids that split, Some(cost) allows
    /// it and adds the signed cost to the objective.
    pub split_costs: Vec<Option<i64>>,
}

/// Measured alternatives for one logical block.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BlockCandidates {
    pub variants: Vec<BlockVariant>,
}

impl BlockCandidates {
    /// Lift line-grid paragraph candidates into the mixed-height model using a
    /// caller-supplied leading for every line.
    #[must_use]
    pub fn from_paragraph_candidates(
        paragraph: &ParagraphCandidates,
        line_height: LayoutUnit,
    ) -> Self {
        Self {
            variants: paragraph
                .variants
                .iter()
                .map(|variant| BlockVariant {
                    demerits: variant.demerits,
                    fragment_heights: vec![line_height; variant.line_count],
                    continuation_prefix: LayoutUnit::ZERO,
                    fragment_reservations: Vec::new(),
                    split_costs: Vec::new(),
                })
                .collect(),
        }
    }
}

/// Structural constraints for one mixed-height block.
///
/// `min_before_break` is the minimum fragment count on a page when the block
/// continues afterward. `min_after_break` is the minimum count on any
/// continuation fragment. Set either penalty to `None` for a hard minimum, or
/// `Some(weight)` to permit each missing fragment at that cost.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BlockPolicy {
    pub keep_together: bool,
    pub keep_with_next: bool,
    pub break_before: bool,
    /// Signed objective cost charged when pagination actually opens a new page
    /// immediately before this block. This is independent of break_before:
    /// callers may price optional boundaries or forced ones as needed.
    pub break_before_cost: i64,
    pub min_before_break: usize,
    pub min_after_break: usize,
    pub before_break_penalty: Option<u64>,
    pub after_break_penalty: Option<u64>,
}

impl Default for BlockPolicy {
    fn default() -> Self {
        Self {
            keep_together: false,
            keep_with_next: false,
            break_before: false,
            break_before_cost: 0,
            min_before_break: 1,
            min_after_break: 1,
            before_break_penalty: None,
            after_break_penalty: None,
        }
    }
}

impl BlockPolicy {
    /// Conventional two-line widow/orphan paragraph policy.
    #[must_use]
    pub const fn paragraph() -> Self {
        Self {
            keep_together: false,
            keep_with_next: false,
            break_before: false,
            break_before_cost: 0,
            min_before_break: 2,
            min_after_break: 2,
            before_break_penalty: None,
            after_break_penalty: None,
        }
    }
}

/// Bounded-search limits for the mixed-height planner.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HeightPaginationLimits {
    pub max_blocks: usize,
    pub max_variants_per_block: usize,
    pub max_candidate_fragments: usize,
    pub max_page_capacity_mpt: usize,
    pub max_nodes: usize,
    pub max_transitions: usize,
}

impl Default for HeightPaginationLimits {
    fn default() -> Self {
        Self {
            max_blocks: 10_000,
            max_variants_per_block: 32,
            max_candidate_fragments: 1_000_000,
            max_page_capacity_mpt: 10_000_000,
            max_nodes: 1_000_000,
            max_transitions: 8_000_000,
        }
    }
}

/// Objective and admission settings.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HeightPaginationOptions {
    pub page_capacity: LayoutUnit,
    pub initial_used: LayoutUnit,
    /// Optional hard publication limit, including an occupied initial page.
    pub max_pages: Option<usize>,
    pub page_cost: u64,
    /// Maximum ragged-page cost. Actual cost is
    /// `gap^2 / capacity^2 * unused_height_cost`, so changing height units does
    /// not change the objective.
    pub unused_height_cost: u64,
    pub penalize_last_page: bool,
    pub limits: HeightPaginationLimits,
}

impl Default for HeightPaginationOptions {
    fn default() -> Self {
        Self {
            page_capacity: LayoutUnit::from_points(720),
            initial_used: LayoutUnit::ZERO,
            max_pages: None,
            page_cost: 50_000,
            unused_height_cost: 10_000,
            penalize_last_page: false,
            limits: HeightPaginationLimits::default(),
        }
    }
}

/// One contiguous fragment range of one chosen block variant on one page.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HeightPageFragment {
    pub block_index: usize,
    pub variant_chosen: usize,
    pub fragment_start: usize,
    pub fragment_end: usize,
    pub page_index: usize,
    /// Height inserted immediately before this fragment on its page, such as
    /// a repeated table header. Zero on a block's first fragment.
    pub prefix_height: LayoutUnit,
    /// Offset where the fragment's own content starts (after prefix_height).
    pub page_offset: LayoutUnit,
    /// Content height only; prefix_height is reported separately.
    pub height: LayoutUnit,
    /// Bottom reservation contributed by this fragment range. This consumes
    /// page capacity but does not move subsequent body baselines downward.
    pub reservation_height: LayoutUnit,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HeightPaginationPlan {
    pub variants: Vec<usize>,
    pub fragments: Vec<HeightPageFragment>,
    pub page_count: usize,
    pub total_demerits: i128,
    pub search_nodes: usize,
    pub search_transitions: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum HeightPaginationError {
    InvalidOptions(&'static str),
    InvalidBlock {
        block_index: usize,
        reason: &'static str,
    },
    BudgetExceeded(&'static str),
    CostOverflow,
    NoFeasibleLayout {
        block_index: usize,
    },
}

impl fmt::Display for HeightPaginationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidOptions(reason) => {
                write!(f, "invalid mixed-height pagination options: {reason}")
            }
            Self::InvalidBlock {
                block_index,
                reason,
            } => write!(f, "invalid mixed-height block {block_index}: {reason}"),
            Self::BudgetExceeded(budget) => {
                write!(f, "mixed-height pagination budget exceeded: {budget}")
            }
            Self::CostOverflow => f.write_str("mixed-height pagination cost overflow"),
            Self::NoFeasibleLayout { block_index } => {
                write!(f, "no feasible mixed-height pagination through block {block_index}")
            }
        }
    }
}

impl std::error::Error for HeightPaginationError {}

#[derive(Clone, Copy, Debug)]
struct State {
    cost: i128,
    closed_pages: usize,
    body_used: usize,
    reserved: usize,
    tail: Option<usize>,
}

impl State {
    fn rank(self) -> (i128, usize) {
        (self.cost, self.closed_pages)
    }
}

type StateKey = (usize, usize, usize, usize);

struct Node {
    previous: Option<usize>,
    fragment: HeightPageFragment,
}

struct Search {
    options: HeightPaginationOptions,
    capacity: usize,
    nodes: Vec<Node>,
    transitions: usize,
}

impl Search {
    fn key(&self, progress: usize, state: State) -> StateKey {
        (
            progress,
            state.body_used,
            state.reserved,
            if self.options.max_pages.is_some() {
                state.closed_pages
            } else {
                0
            },
        )
    }

    fn occupied(&self, state: State) -> Result<usize, HeightPaginationError> {
        state
            .body_used
            .checked_add(state.reserved)
            .ok_or(HeightPaginationError::CostOverflow)
    }

    fn charge(&mut self) -> Result<(), HeightPaginationError> {
        if self.transitions >= self.options.limits.max_transitions {
            return Err(HeightPaginationError::BudgetExceeded("transitions"));
        }
        self.transitions += 1;
        Ok(())
    }

    fn ragged_cost(&self, used: usize) -> Result<i128, HeightPaginationError> {
        let gap = self.capacity - used;
        let gap = i128::try_from(gap).map_err(|_| HeightPaginationError::CostOverflow)?;
        let capacity =
            i128::try_from(self.capacity).map_err(|_| HeightPaginationError::CostOverflow)?;
        gap.checked_mul(gap)
            .and_then(|n| n.checked_mul(i128::from(self.options.unused_height_cost)))
            .and_then(|n| n.checked_div(capacity.checked_mul(capacity)?))
            .ok_or(HeightPaginationError::CostOverflow)
    }

    fn close_page(
        &self,
        mut state: State,
        last: bool,
    ) -> Result<State, HeightPaginationError> {
        let occupied = self.occupied(state)?;
        if occupied == 0 {
            return Ok(state);
        }
        let mut cost = i128::from(self.options.page_cost);
        if !last || self.options.penalize_last_page {
            cost = cost
                .checked_add(self.ragged_cost(occupied)?)
                .ok_or(HeightPaginationError::CostOverflow)?;
        }
        state.cost = state
            .cost
            .checked_add(cost)
            .ok_or(HeightPaginationError::CostOverflow)?;
        state.closed_pages = state
            .closed_pages
            .checked_add(1)
            .ok_or(HeightPaginationError::CostOverflow)?;
        state.body_used = 0;
        state.reserved = 0;
        Ok(state)
    }

    fn retain(
        &mut self,
        frontier: &mut BTreeMap<StateKey, State>,
        progress: usize,
        mut candidate: State,
        fragment: HeightPageFragment,
    ) -> Result<(), HeightPaginationError> {
        let key = self.key(progress, candidate);
        if frontier
            .get(&key)
            .is_some_and(|old| old.rank() <= candidate.rank())
        {
            return Ok(());
        }
        if self.nodes.len() >= self.options.limits.max_nodes {
            return Err(HeightPaginationError::BudgetExceeded("backpointer nodes"));
        }
        let index = self.nodes.len();
        self.nodes.push(Node {
            previous: candidate.tail,
            fragment,
        });
        candidate.tail = Some(index);
        frontier.insert(key, candidate);
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    fn place(
        &mut self,
        state: State,
        block: usize,
        variant_index: usize,
        heights: &[usize],
        reservations: &[usize],
        split_costs: &[Option<i64>],
        continuation_prefix: usize,
        start: usize,
        policy: BlockPolicy,
        completed: &mut BTreeMap<StateKey, State>,
        pending: &mut BTreeMap<StateKey, State>,
    ) -> Result<(), HeightPaginationError> {
        if self
            .options
            .max_pages
            .is_some_and(|limit| state.closed_pages >= limit)
        {
            return Ok(());
        }

        let prefix = if start > 0 { continuation_prefix } else { 0 };
        let Some(content_start) = state.body_used.checked_add(prefix) else {
            return Err(HeightPaginationError::CostOverflow);
        };
        let mut choices = Vec::new();
        let mut height = 0usize;
        let mut reservation = 0usize;
        let mut end = start;
        while end < heights.len() {
            height = height
                .checked_add(heights[end])
                .ok_or(HeightPaginationError::CostOverflow)?;
            reservation = reservation
                .checked_add(reservations[end])
                .ok_or(HeightPaginationError::CostOverflow)?;
            let occupied = content_start
                .checked_add(height)
                .and_then(|n| n.checked_add(state.reserved))
                .and_then(|n| n.checked_add(reservation))
                .ok_or(HeightPaginationError::CostOverflow)?;
            if occupied > self.capacity {
                break;
            }
            end += 1;
            choices.push((end, height, reservation));
        }

        // Longest legal fragments first make equal-cost ties prefer page fill.
        for &(end, fragment_height, fragment_reservation) in choices.iter().rev() {
            self.charge()?;
            let count = end - start;
            let split = end < heights.len();
            if policy.keep_together && split {
                continue;
            }

            let mut penalty = 0_i128;
            if split {
                if !split_costs.is_empty() {
                    let Some(boundary_cost) = split_costs[end - 1] else {
                        continue;
                    };
                    penalty = penalty
                        .checked_add(i128::from(boundary_cost))
                        .ok_or(HeightPaginationError::CostOverflow)?;
                }
                let Some(cost) = violation(
                    count,
                    policy.min_before_break,
                    policy.before_break_penalty,
                )?
                else {
                    continue;
                };
                penalty = penalty
                    .checked_add(cost)
                    .ok_or(HeightPaginationError::CostOverflow)?;
            }
            if start > 0 {
                let Some(cost) = violation(
                    count,
                    policy.min_after_break,
                    policy.after_break_penalty,
                )?
                else {
                    continue;
                };
                penalty = penalty
                    .checked_add(cost)
                    .ok_or(HeightPaginationError::CostOverflow)?;
            }

            let mut candidate = State {
                cost: state
                    .cost
                    .checked_add(penalty)
                    .ok_or(HeightPaginationError::CostOverflow)?,
                body_used: content_start
                    .checked_add(fragment_height)
                    .ok_or(HeightPaginationError::CostOverflow)?,
                reserved: state
                    .reserved
                    .checked_add(fragment_reservation)
                    .ok_or(HeightPaginationError::CostOverflow)?,
                ..state
            };
            let fragment = HeightPageFragment {
                block_index: block,
                variant_chosen: variant_index,
                fragment_start: start,
                fragment_end: end,
                page_index: state.closed_pages,
                prefix_height: LayoutUnit::from_milli_points(
                    i32::try_from(prefix).map_err(|_| HeightPaginationError::CostOverflow)?,
                ),
                page_offset: LayoutUnit::from_milli_points(
                    i32::try_from(content_start)
                        .map_err(|_| HeightPaginationError::CostOverflow)?,
                ),
                height: LayoutUnit::from_milli_points(
                    i32::try_from(fragment_height)
                        .map_err(|_| HeightPaginationError::CostOverflow)?,
                ),
                reservation_height: LayoutUnit::from_milli_points(
                    i32::try_from(fragment_reservation)
                        .map_err(|_| HeightPaginationError::CostOverflow)?,
                ),
            };

            if split {
                candidate = self.close_page(candidate, false)?;
                self.retain(pending, end, candidate, fragment)?;
            } else {
                let occupied = self.occupied(candidate)?;
                self.retain(completed, occupied, candidate, fragment)?;
            }
        }
        Ok(())
    }
}

fn violation(
    count: usize,
    minimum: usize,
    penalty: Option<u64>,
) -> Result<Option<i128>, HeightPaginationError> {
    let missing = minimum.saturating_sub(count);
    if missing == 0 {
        return Ok(Some(0));
    }
    let Some(penalty) = penalty else {
        return Ok(None);
    };
    let cost = i128::try_from(missing)
        .map_err(|_| HeightPaginationError::CostOverflow)?
        .checked_mul(i128::from(penalty))
        .ok_or(HeightPaginationError::CostOverflow)?;
    Ok(Some(cost))
}

fn nonnegative_mpt(
    value: LayoutUnit,
    name: &'static str,
) -> Result<usize, HeightPaginationError> {
    let raw = value.milli_points();
    if raw < 0 {
        return Err(HeightPaginationError::InvalidOptions(name));
    }
    usize::try_from(raw).map_err(|_| HeightPaginationError::CostOverflow)
}

fn validate(
    blocks: &[BlockCandidates],
    policies: &[BlockPolicy],
    options: HeightPaginationOptions,
) -> Result<(usize, usize), HeightPaginationError> {
    let capacity = nonnegative_mpt(options.page_capacity, "page capacity must be positive")?;
    let initial = nonnegative_mpt(options.initial_used, "initial used height must be nonnegative")?;
    if capacity == 0 || initial > capacity {
        return Err(HeightPaginationError::InvalidOptions(
            "page capacity must be positive and contain the initial used height",
        ));
    }
    if options.max_pages == Some(0) {
        return Err(HeightPaginationError::InvalidOptions(
            "max_pages must be positive when supplied",
        ));
    }
    if !policies.is_empty() && policies.len() != blocks.len() {
        return Err(HeightPaginationError::InvalidOptions(
            "policies must be empty or have one entry per block",
        ));
    }
    if blocks.len() > options.limits.max_blocks {
        return Err(HeightPaginationError::BudgetExceeded("blocks"));
    }
    if capacity > options.limits.max_page_capacity_mpt {
        return Err(HeightPaginationError::BudgetExceeded("page capacity"));
    }

    let mut total = 0usize;
    for (block_index, block) in blocks.iter().enumerate() {
        if block.variants.is_empty() {
            return Err(HeightPaginationError::InvalidBlock {
                block_index,
                reason: "no measured variants",
            });
        }
        if block.variants.len() > options.limits.max_variants_per_block {
            return Err(HeightPaginationError::BudgetExceeded("variants per block"));
        }
        for variant in &block.variants {
            if variant.fragment_heights.is_empty() {
                return Err(HeightPaginationError::InvalidBlock {
                    block_index,
                    reason: "variant has no fragments",
                });
            }
            if variant.continuation_prefix.milli_points() < 0 {
                return Err(HeightPaginationError::InvalidBlock {
                    block_index,
                    reason: "continuation prefix height must be nonnegative",
                });
            }
            if !variant.fragment_reservations.is_empty()
                && variant.fragment_reservations.len() != variant.fragment_heights.len()
            {
                return Err(HeightPaginationError::InvalidBlock {
                    block_index,
                    reason: "fragment reservation count must match fragment count",
                });
            }
            if !variant.split_costs.is_empty()
                && variant.split_costs.len() + 1 != variant.fragment_heights.len()
            {
                return Err(HeightPaginationError::InvalidBlock {
                    block_index,
                    reason: "split policy count must be one less than fragment count",
                });
            }
            for &reservation in &variant.fragment_reservations {
                if reservation.milli_points() < 0 {
                    return Err(HeightPaginationError::InvalidBlock {
                        block_index,
                        reason: "fragment reservation height must be nonnegative",
                    });
                }
            }
            for &height in &variant.fragment_heights {
                if height.milli_points() <= 0 {
                    return Err(HeightPaginationError::InvalidBlock {
                        block_index,
                        reason: "fragment height must be positive",
                    });
                }
            }
            total = total
                .checked_add(variant.fragment_heights.len())
                .filter(|&n| n <= options.limits.max_candidate_fragments)
                .ok_or(HeightPaginationError::BudgetExceeded("candidate fragments"))?;
        }
    }
    Ok((capacity, initial))
}

/// Find the exact minimum-cost mixed-height pagination plan.
///
/// The planner is an acyclic shortest-path search. At block boundaries, future
/// behavior depends on occupied page height and, when a hard page limit exists,
/// pages already closed. Inside one chosen variant, continuation states also
/// include the next unplaced fragment index. Only states with identical future
/// constraints are compared for dominance.
///
/// A fragment taller than the page is simply infeasible for that variant; a
/// shorter alternative variant may still win. Callers should scale oversized
/// images or otherwise create a legal measured variant before pagination.
///
/// # Errors
/// Returns a typed error for invalid inputs, infeasibility, checked-arithmetic
/// overflow, or explicit work-budget exhaustion.
pub fn plan_blocks(
    blocks: &[BlockCandidates],
    policies: &[BlockPolicy],
    options: HeightPaginationOptions,
) -> Result<HeightPaginationPlan, HeightPaginationError> {
    let (capacity, initial_used) = validate(blocks, policies, options)?;
    let policy_at = |index| policies.get(index).copied().unwrap_or_default();
    let mut search = Search {
        options,
        capacity,
        nodes: Vec::new(),
        transitions: 0,
    };
    let initial = State {
        cost: 0,
        closed_pages: 0,
        body_used: initial_used,
        reserved: 0,
        tail: None,
    };
    let mut frontier = BTreeMap::from([(search.key(initial_used, initial), initial)]);

    for (block_index, block) in blocks.iter().enumerate() {
        let policy = policy_at(block_index);
        let keep_previous =
            block_index > 0 && policy_at(block_index - 1).keep_with_next;
        let mut completed = BTreeMap::new();

        for (variant_index, variant) in block.variants.iter().enumerate() {
            let mut heights = Vec::with_capacity(variant.fragment_heights.len());
            for &height in &variant.fragment_heights {
                heights.push(
                    usize::try_from(height.milli_points())
                        .map_err(|_| HeightPaginationError::CostOverflow)?,
                );
            }
            let continuation_prefix = usize::try_from(
                variant.continuation_prefix.milli_points(),
            )
            .map_err(|_| HeightPaginationError::CostOverflow)?;
            let reservations = if variant.fragment_reservations.is_empty() {
                vec![0usize; heights.len()]
            } else {
                variant
                    .fragment_reservations
                    .iter()
                    .map(|reservation| {
                        usize::try_from(reservation.milli_points())
                            .map_err(|_| HeightPaginationError::CostOverflow)
                    })
                    .collect::<Result<Vec<_>, _>>()?
            };
            let mut pending = BTreeMap::new();

            for &previous in frontier.values() {
                search.charge()?;
                let state = State {
                    cost: previous
                        .cost
                        .checked_add(i128::from(variant.demerits))
                        .ok_or(HeightPaginationError::CostOverflow)?,
                    ..previous
                };

                let occupied = search.occupied(state)?;
                if !policy.break_before || occupied == 0 {
                    search.place(
                        state,
                        block_index,
                        variant_index,
                        &heights,
                        &reservations,
                        &variant.split_costs,
                        continuation_prefix,
                        0,
                        policy,
                        &mut completed,
                        &mut pending,
                    )?;
                }

                if occupied > 0 && !keep_previous {
                    let mut state = search.close_page(state, false)?;
                    state.cost = state
                        .cost
                        .checked_add(i128::from(policy.break_before_cost))
                        .ok_or(HeightPaginationError::CostOverflow)?;
                    search.place(
                        state,
                        block_index,
                        variant_index,
                        &heights,
                        &reservations,
                        &variant.split_costs,
                        continuation_prefix,
                        0,
                        policy,
                        &mut completed,
                        &mut pending,
                    )?;
                }
            }

            while let Some(((start, _, _, _), state)) = pending.pop_first() {
                search.place(
                    state,
                    block_index,
                    variant_index,
                    &heights,
                    &reservations,
                    &variant.split_costs,
                    continuation_prefix,
                    start,
                    policy,
                    &mut completed,
                    &mut pending,
                )?;
            }
        }

        if completed.is_empty() {
            return Err(HeightPaginationError::NoFeasibleLayout { block_index });
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
    let best = best.ok_or(HeightPaginationError::NoFeasibleLayout { block_index: 0 })?;

    let mut fragments = Vec::new();
    let mut cursor = best.tail;
    while let Some(index) = cursor {
        let node = &search.nodes[index];
        fragments.push(node.fragment);
        cursor = node.previous;
    }
    fragments.reverse();

    let mut variants = vec![0usize; blocks.len()];
    for fragment in &fragments {
        variants[fragment.block_index] = fragment.variant_chosen;
    }

    Ok(HeightPaginationPlan {
        variants,
        fragments,
        page_count: best.closed_pages,
        total_demerits: best.cost,
        search_nodes: search.nodes.len(),
        search_transitions: search.transitions,
    })
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

    use super::*;
    use crate::layout::{ParagraphVariant, LineBreak};

    fn u(value: i32) -> LayoutUnit {
        LayoutUnit::from_milli_points(value)
    }

    fn block(variants: &[(i64, &[i32])]) -> BlockCandidates {
        BlockCandidates {
            variants: variants
                .iter()
                .map(|(demerits, heights)| BlockVariant {
                    demerits: *demerits,
                    fragment_heights: heights.iter().copied().map(u).collect(),
                    continuation_prefix: LayoutUnit::ZERO,
                    fragment_reservations: Vec::new(),
                    split_costs: Vec::new(),
                })
                .collect(),
        }
    }

    fn block_with_prefix(
        demerits: i64,
        heights: &[i32],
        prefix: i32,
    ) -> BlockCandidates {
        BlockCandidates {
            variants: vec![BlockVariant {
                demerits,
                fragment_heights: heights.iter().copied().map(u).collect(),
                continuation_prefix: u(prefix),
                fragment_reservations: Vec::new(),
                split_costs: Vec::new(),
            }],
        }
    }

    fn block_with_reservations(
        demerits: i64,
        heights: &[i32],
        reservations: &[i32],
    ) -> BlockCandidates {
        BlockCandidates {
            variants: vec![BlockVariant {
                demerits,
                fragment_heights: heights.iter().copied().map(u).collect(),
                continuation_prefix: LayoutUnit::ZERO,
                fragment_reservations: reservations.iter().copied().map(u).collect(),
                split_costs: Vec::new(),
            }],
        }
    }

    fn block_with_splits(
        demerits: i64,
        heights: &[i32],
        split_costs: Vec<Option<i64>>,
    ) -> BlockCandidates {
        BlockCandidates {
            variants: vec![BlockVariant {
                demerits,
                fragment_heights: heights.iter().copied().map(u).collect(),
                continuation_prefix: LayoutUnit::ZERO,
                fragment_reservations: Vec::new(),
                split_costs,
            }],
        }
    }

    fn options(capacity: i32) -> HeightPaginationOptions {
        HeightPaginationOptions {
            page_capacity: u(capacity),
            page_cost: 100,
            unused_height_cost: 100,
            ..HeightPaginationOptions::default()
        }
    }

    #[test]
    fn paragraph_adapter_preserves_variants_and_line_counts() {
        let paragraph = ParagraphCandidates {
            variants: vec![
                ParagraphVariant { line_count: 2, demerits: -4, lines: Vec::<LineBreak>::new() },
                ParagraphVariant { line_count: 3, demerits: 7, lines: Vec::<LineBreak>::new() },
            ],
        };
        let lifted = BlockCandidates::from_paragraph_candidates(&paragraph, u(12));
        assert_eq!(lifted.variants.len(), 2);
        assert_eq!(lifted.variants[0].demerits, -4);
        assert_eq!(lifted.variants[0].fragment_heights, vec![u(12); 2]);
        assert_eq!(lifted.variants[1].fragment_heights, vec![u(12); 3]);
    }

    #[test]
    fn variant_choice_accounts_for_real_height_not_fragment_count() {
        let blocks = [
            block(&[(0, &[60, 60]), (9, &[45, 45])]),
            block(&[(0, &[10])]),
        ];
        let plan = plan_blocks(&blocks, &[], options(100)).unwrap();
        assert_eq!(plan.variants, vec![1, 0]);
        assert_eq!(plan.page_count, 1);
        assert_eq!(plan.fragments.len(), 2);
        assert_eq!(plan.total_demerits, 109);
    }

    #[test]
    fn split_uses_exact_fragment_heights_and_two_item_minima() {
        let blocks = [block(&[(0, &[30, 30, 30, 30])])];
        let policies = [BlockPolicy::paragraph()];
        let plan = plan_blocks(&blocks, &policies, options(100)).unwrap();
        let ranges: Vec<_> = plan
            .fragments
            .iter()
            .map(|f| (f.fragment_start, f.fragment_end, f.height.milli_points()))
            .collect();
        assert_eq!(ranges, vec![(0, 2, 60), (2, 4, 60)]);
        assert_eq!(plan.page_count, 2);
    }

    #[test]
    fn continuation_prefix_consumes_capacity_and_is_reconstructed() {
        let blocks = [block_with_prefix(0, &[40, 40, 40, 40], 20)];
        let plan = plan_blocks(&blocks, &[], options(100)).unwrap();
        assert_eq!(plan.fragments.len(), 2);
        assert_eq!(plan.fragments[0].fragment_start, 0);
        assert_eq!(plan.fragments[0].fragment_end, 2);
        assert_eq!(plan.fragments[0].prefix_height, LayoutUnit::ZERO);
        assert_eq!(plan.fragments[0].page_offset, LayoutUnit::ZERO);
        assert_eq!(plan.fragments[0].height, u(80));
        assert_eq!(plan.fragments[1].fragment_start, 2);
        assert_eq!(plan.fragments[1].fragment_end, 4);
        assert_eq!(plan.fragments[1].prefix_height, u(20));
        assert_eq!(plan.fragments[1].page_offset, u(20));
        assert_eq!(plan.fragments[1].height, u(80));
        assert_eq!(plan.page_count, 2);
    }

    #[test]
    fn bottom_reservations_consume_capacity_without_shifting_body_offsets() {
        let blocks = [
            block_with_reservations(0, &[30], &[20]),
            block(&[(0, &[30])]),
        ];
        let plan = plan_blocks(&blocks, &[], options(100)).unwrap();
        assert_eq!(plan.page_count, 1);
        assert_eq!(plan.fragments[0].page_offset, u(0));
        assert_eq!(plan.fragments[0].reservation_height, u(20));
        assert_eq!(plan.fragments[1].page_offset, u(30));
        assert_eq!(plan.fragments[1].reservation_height, LayoutUnit::ZERO);

        let forced = [
            block_with_reservations(0, &[60], &[30]),
            block(&[(0, &[20])]),
        ];
        let plan = plan_blocks(&forced, &[], options(100)).unwrap();
        assert_eq!(plan.page_count, 2, "reservation must reduce usable body capacity");
        assert_eq!(plan.fragments[1].page_index, 1);
        assert_eq!(plan.fragments[1].page_offset, LayoutUnit::ZERO);
    }

    #[test]
    fn per_boundary_policy_forbids_and_prices_splits() {
        let hard = [block_with_splits(
            0,
            &[40, 40],
            vec![None],
        )];
        assert!(matches!(
            plan_blocks(&hard, &[], options(60)),
            Err(HeightPaginationError::NoFeasibleLayout { .. })
        ));

        let priced = [block_with_splits(
            0,
            &[40, 40, 40],
            vec![Some(500), Some(0)],
        )];
        let priced_plan = plan_blocks(&priced, &[], options(100)).unwrap();
        assert_eq!(
            priced_plan
                .fragments
                .iter()
                .map(|f| (f.fragment_start, f.fragment_end))
                .collect::<Vec<_>>(),
            vec![(0, 2), (2, 3)],
            "boundary-specific cost should steer the exact plan"
        );
    }

    #[test]
    fn block_boundary_cost_is_charged_only_when_page_opens_there() {
        let blocks = [block(&[(0, &[60])]), block(&[(0, &[60])])];
        let policies = [
            BlockPolicy::default(),
            BlockPolicy {
                break_before_cost: 777,
                ..BlockPolicy::default()
            },
        ];
        let plan = plan_blocks(&blocks, &policies, options(100)).unwrap();
        assert_eq!(plan.page_count, 2);
        assert_eq!(plan.total_demerits, 993);

        let fits = [block(&[(0, &[30])]), block(&[(0, &[30])])];
        let plan = plan_blocks(&fits, &policies, options(100)).unwrap();
        assert_eq!(plan.page_count, 1);
        assert_eq!(plan.total_demerits, 100);
    }

    #[test]
    fn keep_with_next_and_break_before_conflict_fail_closed() {
        let blocks = [block(&[(0, &[30])]), block(&[(0, &[30])])];
        let policies = [
            BlockPolicy {
                keep_with_next: true,
                ..BlockPolicy::default()
            },
            BlockPolicy {
                break_before: true,
                ..BlockPolicy::default()
            },
        ];
        assert!(matches!(
            plan_blocks(&blocks, &policies, options(100)),
            Err(HeightPaginationError::NoFeasibleLayout { .. })
        ));
    }

    #[test]
    fn oversized_variant_can_lose_to_legal_alternative() {
        let blocks = [block(&[(0, &[120]), (5, &[80])])];
        let plan = plan_blocks(&blocks, &[], options(100)).unwrap();
        assert_eq!(plan.variants, vec![1]);
        assert_eq!(plan.page_count, 1);
        assert_eq!(plan.total_demerits, 105);
    }

    #[test]
    fn hard_page_limit_retains_costlier_shorter_height_path() {
        let blocks = [
            block(&[(0, &[100, 100]), (200, &[100])]),
            block(&[(0, &[100])]),
        ];
        let unconstrained = plan_blocks(&blocks, &[], options(100)).unwrap();
        assert_eq!(unconstrained.variants, vec![0, 0]);
        assert_eq!(unconstrained.page_count, 3);

        let bounded_options = HeightPaginationOptions {
            max_pages: Some(2),
            ..options(100)
        };
        let bounded = plan_blocks(&blocks, &[], bounded_options).unwrap();
        assert_eq!(bounded.variants, vec![1, 0]);
        assert_eq!(bounded.page_count, 2);
        assert_eq!(bounded.total_demerits, 400);
    }

    #[test]
    fn initial_used_height_counts_as_the_first_page() {
        let blocks = [block(&[(0, &[40])])];
        let one_page = HeightPaginationOptions {
            initial_used: u(60),
            max_pages: Some(1),
            ..options(100)
        };
        let plan = plan_blocks(&blocks, &[], one_page).unwrap();
        assert_eq!(plan.page_count, 1);
        assert_eq!(plan.fragments[0].page_offset, u(60));

        let impossible = HeightPaginationOptions {
            initial_used: u(70),
            ..one_page
        };
        assert!(matches!(
            plan_blocks(&blocks, &[], impossible),
            Err(HeightPaginationError::NoFeasibleLayout { .. })
        ));
    }

    #[test]
    fn invalid_inputs_and_budgets_are_typed() {
        let empty = BlockCandidates { variants: Vec::new() };
        assert!(matches!(
            plan_blocks(&[empty], &[], options(100)),
            Err(HeightPaginationError::InvalidBlock { .. })
        ));
        let bad = block(&[(0, &[0])]);
        assert!(matches!(
            plan_blocks(&[bad], &[], options(100)),
            Err(HeightPaginationError::InvalidBlock { .. })
        ));
        let limits = HeightPaginationLimits {
            max_candidate_fragments: 1,
            ..HeightPaginationLimits::default()
        };
        let opts = HeightPaginationOptions {
            limits,
            ..options(100)
        };
        assert!(matches!(
            plan_blocks(&[block(&[(0, &[10, 10])])], &[], opts),
            Err(HeightPaginationError::BudgetExceeded("candidate fragments"))
        ));
    }

    fn page_cost(opts: HeightPaginationOptions, used: usize, last: bool) -> i128 {
        let cap = opts.page_capacity.milli_points() as i128;
        let gap = cap - used as i128;
        i128::from(opts.page_cost)
            + if !last || opts.penalize_last_page {
                gap * gap * i128::from(opts.unused_height_cost) / (cap * cap)
            } else {
                0
            }
    }

    // Independent exhaustive oracle for small inputs. It does not use frontier
    // dominance or backpointers; every variant and legal split is enumerated.
    fn oracle(
        input: &[BlockCandidates],
        policies: &[BlockPolicy],
        opts: HeightPaginationOptions,
    ) -> Option<(i128, usize)> {
        #[allow(clippy::too_many_arguments)]
        fn visit(
            input: &[BlockCandidates],
            policies: &[BlockPolicy],
            opts: HeightPaginationOptions,
            block_index: usize,
            variant: Option<usize>,
            start: usize,
            body_used: usize,
            reserved: usize,
            pages: usize,
            cost: i128,
            best: &mut Option<(i128, usize)>,
        ) {
            let capacity = opts.page_capacity.milli_points() as usize;
            let occupied = body_used + reserved;
            if block_index == input.len() {
                let page_count = pages + usize::from(occupied > 0);
                if opts.max_pages.is_some_and(|limit| page_count > limit) {
                    return;
                }
                let rank = (
                    cost
                        + if occupied > 0 {
                            page_cost(opts, occupied, true)
                        } else {
                            0
                        },
                    page_count,
                );
                if best.is_none_or(|old| rank < old) {
                    *best = Some(rank);
                }
                return;
            }
            if opts
                .max_pages
                .is_some_and(|limit| pages >= limit && occupied == 0)
            {
                return;
            }

            let policy = policies.get(block_index).copied().unwrap_or_default();
            let keep_previous = block_index > 0
                && policies
                    .get(block_index - 1)
                    .is_some_and(|policy| policy.keep_with_next);

            if variant.is_none() {
                for variant_index in 0..input[block_index].variants.len() {
                    let next_cost =
                        cost + i128::from(input[block_index].variants[variant_index].demerits);
                    if !policy.break_before || occupied == 0 {
                        visit(
                            input,
                            policies,
                            opts,
                            block_index,
                            Some(variant_index),
                            0,
                            body_used,
                            reserved,
                            pages,
                            next_cost,
                            best,
                        );
                    }
                    if occupied > 0 && !keep_previous {
                        visit(
                            input,
                            policies,
                            opts,
                            block_index,
                            Some(variant_index),
                            0,
                            0,
                            0,
                            pages + 1,
                            next_cost
                                + page_cost(opts, occupied, false)
                                + i128::from(policy.break_before_cost),
                            best,
                        );
                    }
                }
                return;
            }

            let variant_index = variant.unwrap();
            let variant = &input[block_index].variants[variant_index];
            let prefix = if start > 0 {
                variant.continuation_prefix.milli_points() as usize
            } else {
                0
            };
            let content_start = body_used + prefix;
            if content_start + reserved > capacity {
                return;
            }

            let mut body_height = 0usize;
            let mut reservation_height = 0usize;
            for end in start..variant.fragment_heights.len() {
                body_height += variant.fragment_heights[end].milli_points() as usize;
                if !variant.fragment_reservations.is_empty() {
                    reservation_height +=
                        variant.fragment_reservations[end].milli_points() as usize;
                }
                let occupied_after =
                    content_start + body_height + reserved + reservation_height;
                if occupied_after > capacity {
                    break;
                }

                let count = end + 1 - start;
                let split = end + 1 < variant.fragment_heights.len();
                if policy.keep_together && split {
                    continue;
                }
                let mut penalty = 0i128;
                if split && !variant.split_costs.is_empty() {
                    let Some(boundary_cost) = variant.split_costs[end] else {
                        continue;
                    };
                    penalty += i128::from(boundary_cost);
                }
                let mut legal = true;
                for (active, minimum, weight) in [
                    (split, policy.min_before_break, policy.before_break_penalty),
                    (start > 0, policy.min_after_break, policy.after_break_penalty),
                ] {
                    let missing = minimum.saturating_sub(count);
                    if active && missing > 0 {
                        if let Some(weight) = weight {
                            penalty += missing as i128 * i128::from(weight);
                        } else {
                            legal = false;
                        }
                    }
                }
                if !legal {
                    continue;
                }

                let next_body = content_start + body_height;
                let next_reserved = reserved + reservation_height;
                if split {
                    visit(
                        input,
                        policies,
                        opts,
                        block_index,
                        Some(variant_index),
                        end + 1,
                        0,
                        0,
                        pages + 1,
                        cost + penalty + page_cost(opts, next_body + next_reserved, false),
                        best,
                    );
                } else {
                    visit(
                        input,
                        policies,
                        opts,
                        block_index + 1,
                        None,
                        0,
                        next_body,
                        next_reserved,
                        pages,
                        cost + penalty,
                        best,
                    );
                }
            }
        }

        let mut best = None;
        visit(
            input,
            policies,
            opts,
            0,
            None,
            0,
            opts.initial_used.milli_points() as usize,
            0,
            0,
            0,
            &mut best,
        );
        best
    }

    #[test]
    fn exhaustive_small_height_cases_match_unpruned_oracle() {
        for capacity in 3..=7 {
            for a in 1..=4 {
                for b in 1..=4 {
                    for limit in 1_usize..=3 {
                        let first = if (a + b + limit as i32) % 2 == 0 {
                            BlockCandidates {
                                variants: vec![
                                    BlockVariant {
                                        demerits: -3,
                                        fragment_heights: vec![u(a), u(b)],
                                        continuation_prefix: LayoutUnit::ZERO,
                                        fragment_reservations: vec![u(1), LayoutUnit::ZERO],
                                        split_costs: vec![Some(1)],
                                    },
                                    BlockVariant {
                                        demerits: 2,
                                        fragment_heights: vec![u(a + 1)],
                                        continuation_prefix: LayoutUnit::ZERO,
                                        fragment_reservations: Vec::new(),
                                        split_costs: Vec::new(),
                                    },
                                ],
                            }
                        } else {
                            block(&[(-3, &[a, b]), (2, &[a + 1])])
                        };
                        let input = [first, block(&[(0, &[b])])];
                        let opts = HeightPaginationOptions {
                            max_pages: Some(limit),
                            initial_used: u(1),
                            ..options(capacity)
                        };
                        let expected = oracle(&input, &[], opts);
                        let actual = plan_blocks(&input, &[], opts);
                        match (expected, actual) {
                            (Some(rank), Ok(plan)) => {
                                assert_eq!((plan.total_demerits, plan.page_count), rank);
                            }
                            (None, Err(HeightPaginationError::NoFeasibleLayout { .. })) => {}
                            other => panic!(
                                "height oracle mismatch cap={capacity} a={a} b={b} limit={limit}: {other:?}"
                            ),
                        }
                    }
                }
            }
        }
    }
}
