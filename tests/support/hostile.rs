//! Bounded deterministic byte generation and failure-preserving reduction.

pub struct HostileByteGenerator {
    state: u64,
    max_len: usize,
}
impl HostileByteGenerator {
    pub fn new(seed: u64, max_len: usize) -> Self {
        Self {
            state: seed,
            max_len,
        }
    }
    fn next(&mut self) -> u64 {
        self.state = self.state.wrapping_mul(6364136223846793005).wrapping_add(1);
        self.state >> 32
    }
    pub fn generate(&mut self) -> Vec<u8> {
        let len = (self.next() as usize) % (self.max_len + 1);
        (0..len).map(|_| self.next() as u8).collect()
    }
}

pub struct TerminationBudget(usize);
impl TerminationBudget {
    pub fn attempts(limit: usize) -> Self {
        Self(limit)
    }
}
pub struct Report {
    pub minimized: Vec<u8>,
    pub attempts: usize,
    pub classification_preserved: bool,
    pub exhausted_budget: bool,
}

pub fn minimize<T: Eq>(
    input: &[u8],
    budget: TerminationBudget,
    classify: &mut impl FnMut(&[u8]) -> Option<T>,
) -> Report {
    let original = classify(input);
    let mut report = Report {
        minimized: input.to_vec(),
        attempts: 0,
        classification_preserved: original.is_some(),
        exhausted_budget: false,
    };
    if original.is_none() {
        return report;
    }
    let mut at = 0;
    while report.minimized.len() > 1 && at < report.minimized.len() {
        if report.attempts == budget.0 {
            report.exhausted_budget = true;
            break;
        }
        let mut candidate = report.minimized.clone();
        candidate.remove(at);
        report.attempts += 1;
        if classify(&candidate) == original {
            report.minimized = candidate;
            at = 0;
        } else {
            at += 1;
        }
    }
    report.classification_preserved = classify(&report.minimized) == original;
    report
}
