//! Bounded exact body pagination with a declared footnote continuation policy.
//!
//! The state includes unfinished notes, not just the next body row. Every edge
//! retains its complete physical allocation so counting and painting can replay
//! the same result. Heights and costs are integers; measurement belongs to the
//! caller. The search optimizes body boundaries under the allocation policy
//! below, not arbitrary permutations or splits of note text.

use std::collections::BTreeMap;
use std::ops::Range;

#[derive(Clone, Debug)]
pub(crate) struct Row {
    pub height: i64,
    pub continuation_prefix: i64,
    pub block: usize,
}

#[derive(Clone, Debug)]
pub(crate) struct Block {
    pub range: Range<usize>,
    pub min_before_break: usize,
    pub min_after_break: usize,
}

/// An indivisible measured note unit, e.g. a prose row or a complete table row.
/// A fragment's height is its units' sum plus the first prefix and last suffix.
/// Prefixes include repeated table headers and top ink clearance.
#[derive(Clone, Debug)]
pub(crate) struct Unit {
    pub height: i64,
    pub prefix: i64,
    pub suffix: i64,
}

#[derive(Clone, Debug)]
pub(crate) struct Note {
    pub first_reference: usize,
    pub units: Vec<Unit>,
}

#[derive(Clone, Debug)]
pub(crate) struct Input {
    pub rows: Vec<Row>,
    pub blocks: Vec<Block>,
    /// One entry per body boundary, including zero and the end. None forbids it.
    pub break_costs: Vec<Option<i64>>,
    pub forced: Vec<bool>,
    /// First-citation order, including the source order of simultaneous marks.
    pub notes: Vec<Note>,
    pub separator: i64,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct Limits {
    pub max_blocks: usize,
    pub max_fragments: usize,
    pub max_capacity: i64,
    pub max_nodes: usize,
    pub max_work: usize,
    pub max_retained_ranges: usize,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct Options {
    pub capacity: i64,
    pub page_cost: u64,
    pub unused_height_cost: u64,
    pub limits: Limits,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Error {
    Invalid(&'static str),
    Budget(&'static str),
    Infeasible { body_line: usize },
    CostOverflow,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Fragment {
    pub note: usize,
    pub start: usize,
    pub end: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Page {
    pub body: Range<usize>,
    pub fragments: Vec<Fragment>,
    pub reserve: i64,
}

#[derive(Debug)]
pub(crate) struct Plan {
    pub pages: Vec<Page>,
    pub total_demerits: i128,
    pub search_nodes: usize,
    pub search_work: usize,
}

/// (note index, first unpainted unit), always in first-citation order.
type Pending = Vec<(usize, usize)>;
type Key = (usize, Pending);

#[derive(Clone, Copy)]
struct State {
    cost: i128,
    pages: usize,
    tail: Option<usize>,
}

impl State {
    fn rank(self) -> (i128, usize) {
        (self.cost, self.pages)
    }
}

struct Node {
    previous: Option<usize>,
    page: Page,
}

struct Search {
    options: Options,
    work: usize,
    retained_ranges: usize,
    nodes: Vec<Node>,
}

impl Search {
    fn charge(&mut self, work: usize) -> Result<(), Error> {
        self.work = self
            .work
            .checked_add(work)
            .ok_or(Error::Budget("continuation work"))?;
        if self.work > self.options.limits.max_work {
            return Err(Error::Budget("continuation work"));
        }
        Ok(())
    }

    fn retain(&mut self, ranges: usize) -> Result<(), Error> {
        self.retained_ranges = self
            .retained_ranges
            .checked_add(ranges)
            .ok_or(Error::Budget("continuation ranges"))?;
        if self.retained_ranges > self.options.limits.max_retained_ranges {
            return Err(Error::Budget("continuation ranges"));
        }
        Ok(())
    }

    fn append(&mut self, previous: State, page: Page, cost: i128) -> Result<State, Error> {
        if self.nodes.len() >= self.options.limits.max_nodes {
            return Err(Error::Budget("continuation nodes"));
        }
        self.retain(page.fragments.len())?;
        let tail = self.nodes.len();
        self.nodes.push(Node {
            previous: previous.tail,
            page,
        });
        Ok(State {
            cost: previous.cost.checked_add(cost).ok_or(Error::CostOverflow)?,
            pages: previous.pages.checked_add(1).ok_or(Error::CostOverflow)?,
            tail: Some(tail),
        })
    }

    fn page_cost(&self, used: i64, final_page: bool, boundary: i64) -> Result<i128, Error> {
        let options = self.options;
        let gap = i128::from(options.capacity - used);
        let void = if final_page {
            0
        } else {
            gap.checked_mul(gap)
                .and_then(|square| square.checked_mul(i128::from(options.unused_height_cost)))
                .ok_or(Error::CostOverflow)?
                / (i128::from(options.capacity) * i128::from(options.capacity))
        };
        i128::from(options.page_cost)
            .checked_add(void)
            .and_then(|cost| cost.checked_add(i128::from(boundary)))
            .ok_or(Error::CostOverflow)
    }
}

struct Measured<'a> {
    input: &'a Input,
    body_heights: Vec<i64>,
    note_heights: Vec<Vec<i64>>,
    next_forced: Vec<usize>,
}

fn height_sum(previous: i64, height: i64) -> Result<i64, Error> {
    previous
        .checked_add(height)
        .ok_or(Error::Invalid("height overflow"))
}

impl<'a> Measured<'a> {
    fn new(input: &'a Input, options: Options) -> Result<Self, Error> {
        if options.capacity <= 0 || input.separator < 0 {
            return Err(Error::Invalid("invalid continuation page geometry"));
        }
        if options.capacity > options.limits.max_capacity {
            return Err(Error::Budget("continuation page capacity"));
        }
        let count = input.rows.len();
        if input.blocks.len() > options.limits.max_blocks {
            return Err(Error::Budget("continuation blocks"));
        }
        let fragments = input.notes.iter().try_fold(count, |count, note| {
            count
                .checked_add(note.units.len())
                .ok_or(Error::Budget("continuation fragments"))
        })?;
        if fragments > options.limits.max_fragments {
            return Err(Error::Budget("continuation fragments"));
        }
        if input.break_costs.len() != count + 1 || input.forced.len() != count + 1 {
            return Err(Error::Invalid("body boundary lengths do not match"));
        }
        let mut covered = 0;
        for (index, block) in input.blocks.iter().enumerate() {
            if block.range.start != covered
                || block.range.end <= covered
                || block.range.end > count
                || block.min_before_break == 0
                || block.min_after_break == 0
                || input.rows[block.range.clone()]
                    .iter()
                    .any(|row| row.block != index)
            {
                return Err(Error::Invalid("invalid body block partition"));
            }
            covered = block.range.end;
        }
        if covered != count {
            return Err(Error::Invalid("body blocks do not cover every row"));
        }
        let mut body_heights = Vec::with_capacity(count + 1);
        body_heights.push(0);
        for row in &input.rows {
            if row.height <= 0 || row.continuation_prefix < 0 {
                return Err(Error::Invalid("invalid body row height"));
            }
            let previous = body_heights.last().copied().unwrap_or(0);
            body_heights.push(height_sum(previous, row.height)?);
        }
        let mut note_heights = Vec::with_capacity(input.notes.len());
        let mut previous_reference = 0;
        for note in &input.notes {
            if note.units.is_empty()
                || note.first_reference >= count
                || note.first_reference < previous_reference
            {
                return Err(Error::Invalid("invalid first-citation note order"));
            }
            previous_reference = note.first_reference;
            let mut heights = Vec::with_capacity(note.units.len() + 1);
            heights.push(0);
            for unit in &note.units {
                if unit.height <= 0 || unit.prefix < 0 || unit.suffix < 0 {
                    return Err(Error::Invalid("invalid note unit height"));
                }
                let previous = heights.last().copied().unwrap_or(0);
                heights.push(height_sum(previous, unit.height)?);
            }
            note_heights.push(heights);
        }
        let mut next_forced = vec![count; count];
        let mut next = count;
        for start in (0..count).rev() {
            next_forced[start] = next;
            if input.forced[start] {
                next = start;
            }
        }
        Ok(Self {
            input,
            body_heights,
            note_heights,
            next_forced,
        })
    }

    fn body_height(&self, start: usize, end: usize) -> Result<i64, Error> {
        height_sum(
            self.body_heights[end] - self.body_heights[start],
            self.input.rows[start].continuation_prefix,
        )
    }

    fn legal(&self, start: usize, end: usize) -> bool {
        if end > self.next_forced[start] || self.input.break_costs[end].is_none() {
            return false;
        }
        let first = &self.input.blocks[self.input.rows[start].block];
        if start > first.range.start && end.min(first.range.end) - start < first.min_after_break {
            return false;
        }
        let last = &self.input.blocks[self.input.rows[end - 1].block];
        end >= last.range.end || end - start.max(last.range.start) >= last.min_before_break
    }

    fn note_height(&self, fragment: &Fragment) -> Result<i64, Error> {
        if fragment.start == fragment.end {
            return Ok(0);
        }
        let note = &self.input.notes[fragment.note];
        let prefix = &self.note_heights[fragment.note];
        height_sum(
            height_sum(
                prefix[fragment.end] - prefix[fragment.start],
                note.units[fragment.start].prefix,
            )?,
            note.units[fragment.end - 1].suffix,
        )
    }

    fn extend(
        &self,
        fragment: &mut Fragment,
        budget: i64,
        search: &mut Search,
    ) -> Result<(), Error> {
        while fragment.end < self.input.notes[fragment.note].units.len() {
            search.charge(1)?;
            let candidate = Fragment {
                end: fragment.end + 1,
                ..*fragment
            };
            if self.note_height(&candidate)? > budget {
                break;
            }
            fragment.end += 1;
        }
        Ok(())
    }

    /// New mandatory fragments precede discretionary allocation. Older pending
    /// notes consume the remaining budget in order, then the fresh notes grow.
    /// A mandatory first fragment may exceed the preferred share; the caller
    /// still requires the complete body and note region to fit the actual page.
    fn allocate(
        &self,
        pending: &Pending,
        body: Range<usize>,
        budget: i64,
        search: &mut Search,
    ) -> Result<(Vec<Fragment>, Pending, i64), Error> {
        let notes = &self.input.notes;
        let from = notes.partition_point(|note| note.first_reference < body.start);
        let to = notes.partition_point(|note| note.first_reference < body.end);
        search.charge(pending.len().saturating_add(to - from))?;
        let mut fresh = Vec::with_capacity(to - from);
        let mut used = 0;
        for note in from..to {
            let fragment = Fragment {
                note,
                start: 0,
                end: 1,
            };
            used = height_sum(used, self.note_height(&fragment)?)?;
            fresh.push(fragment);
        }
        let available = (budget - self.input.separator).max(0);
        let mut consumed = 0;
        let mut continued = None;
        let mut fragments = Vec::new();
        for &(note, start) in pending {
            let mut fragment = Fragment {
                note,
                start,
                end: start,
            };
            self.extend(&mut fragment, (available - used).max(0), search)?;
            if fragment.start == fragment.end {
                break;
            }
            used = height_sum(used, self.note_height(&fragment)?)?;
            consumed += 1;
            if fragment.end < notes[note].units.len() {
                continued = Some((note, fragment.end));
            }
            fragments.push(fragment);
            if continued.is_some() {
                break;
            }
        }
        let mut remaining = Vec::with_capacity(pending.len() - consumed + fresh.len() + 1);
        remaining.extend(continued);
        remaining.extend_from_slice(&pending[consumed..]);
        for fragment in &mut fresh {
            let before = self.note_height(fragment)?;
            self.extend(
                fragment,
                height_sum(before, (available - used).max(0))?,
                search,
            )?;
            used = height_sum(used - before, self.note_height(fragment)?)?;
            if fragment.end < notes[fragment.note].units.len() {
                remaining.push((fragment.note, fragment.end));
            }
        }
        fragments.extend(fresh);
        let reserve = if fragments.is_empty() {
            0
        } else {
            height_sum(self.input.separator, used)?
        };
        Ok((fragments, remaining, reserve))
    }
}

/// Exhaust every admitted body boundary. Equal states have identical futures,
/// so only their least-cost predecessor survives. Body edges strictly advance;
/// deterministic note-only tails are drained separately, avoiding an invalid
/// lexicographic ordering assumption about pending note indices.
pub(crate) fn plan(input: &Input, options: Options) -> Result<Plan, Error> {
    let measured = Measured::new(input, options)?;
    let mut search = Search {
        options,
        work: 0,
        retained_ranges: 0,
        nodes: Vec::new(),
    };
    let mut frontier = BTreeMap::<Key, State>::new();
    let initial = State {
        cost: 0,
        pages: 0,
        tail: None,
    };
    frontier.insert((0, Vec::new()), initial);
    let mut best: Option<State> = None;
    let count = input.rows.len();
    let mut furthest = 0;
    'states: while let Some(((start, pending), state)) = frontier.pop_first() {
        search.charge(1)?;
        search.retained_ranges -= pending.len();
        furthest = furthest.max(start);
        if start == count {
            let mut state = state;
            let mut pending = pending;
            while !pending.is_empty() {
                let (fragments, remaining, reserve) =
                    measured.allocate(&pending, count..count, options.capacity, &mut search)?;
                if fragments.is_empty() || reserve > options.capacity {
                    // Another body partition may have consumed this unit
                    // inside an earlier fragment with a different prefix.
                    continue 'states;
                }
                let cost = search.page_cost(reserve, remaining.is_empty(), 0)?;
                state = search.append(
                    state,
                    Page {
                        body: count..count,
                        fragments,
                        reserve,
                    },
                    cost,
                )?;
                pending = remaining;
            }
            if best.is_none_or(|best| state.rank() < best.rank()) {
                best = Some(state);
            }
            continue;
        }
        for end in (start + 1)..=measured.next_forced[start] {
            search.charge(1)?;
            let height = measured.body_height(start, end)?;
            if height > options.capacity {
                break;
            }
            if !measured.legal(start, end) {
                continue;
            }
            let preferred_share = options.capacity / 5 * 2 + options.capacity % 5 * 2 / 5;
            let preferred = preferred_share.min(options.capacity - height);
            let (fragments, remaining, reserve) =
                measured.allocate(&pending, start..end, preferred, &mut search)?;
            let used = height_sum(height, reserve)?;
            if used > options.capacity {
                continue;
            }
            let final_page = end == count && remaining.is_empty();
            let cost = search.page_cost(used, final_page, input.break_costs[end].unwrap_or(0))?;
            let rank = (
                state.cost.checked_add(cost).ok_or(Error::CostOverflow)?,
                state.pages.checked_add(1).ok_or(Error::CostOverflow)?,
            );
            let key = (end, remaining);
            if frontier
                .get(&key)
                .is_some_and(|previous| previous.rank() <= rank)
            {
                continue;
            }
            if !frontier.contains_key(&key) {
                search.retain(key.1.len())?;
            }
            let next = search.append(
                state,
                Page {
                    body: start..end,
                    fragments,
                    reserve,
                },
                cost,
            )?;
            frontier.insert(key, next);
        }
    }
    let best = best.ok_or(Error::Infeasible {
        body_line: furthest,
    })?;
    let mut pages = Vec::with_capacity(best.pages);
    let mut tail = best.tail;
    while let Some(index) = tail {
        let node = &search.nodes[index];
        pages.push(node.page.clone());
        tail = node.previous;
    }
    pages.reverse();
    Ok(Plan {
        pages,
        total_demerits: best.cost,
        search_nodes: search.nodes.len(),
        search_work: search.work,
    })
}

#[cfg(test)]
#[path = "continuation_tests.rs"]
mod tests;
