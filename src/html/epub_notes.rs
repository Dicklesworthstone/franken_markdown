//! EPUB-owned note targets share the HTML renderer's actual heading inventory.
//! Keep legacy identifiers when available, and reserve all of them before
//! allocating replacements so one note cannot take another note's target.

use std::collections::BTreeSet;

use super::HeadingIdentifiers;

pub(super) struct Anchors {
    headings: BTreeSet<String>,
    occupied: BTreeSet<String>,
    notes: Vec<(String, String)>,
}

impl Anchors {
    pub(super) fn new(inventory: HeadingIdentifiers) -> Self {
        let headings: BTreeSet<_> = inventory.headings.into_iter().flatten().collect();
        let mut occupied = inventory.other_anchors;
        occupied.extend(headings.iter().cloned());
        Self {
            headings,
            occupied,
            notes: Vec::new(),
        }
    }

    pub(super) fn push(&mut self, id: &str, number: usize) {
        let body = self.allocate(format!("fn-{id}"));
        let reference = self.allocate(format!("fnref-{number}"));
        self.notes.push((body, reference));
    }

    fn allocate(&mut self, base: String) -> String {
        if !self.headings.contains(&base) {
            return base;
        }
        // Each base is allocated once. Work is bounded by the source-owned
        // occupied identifiers, not by the number of repeated citations.
        let mut suffix = 2usize;
        loop {
            let candidate = format!("{base}-{suffix}");
            if self.occupied.insert(candidate.clone()) {
                return candidate;
            }
            suffix += 1;
        }
    }

    pub(super) fn body(&self, number: usize) -> Option<&str> {
        self.notes
            .get(number.checked_sub(1)?)
            .map(|note| note.0.as_str())
    }

    pub(super) fn reference(&self, number: usize) -> Option<&str> {
        self.notes
            .get(number.checked_sub(1)?)
            .map(|note| note.1.as_str())
    }
}
