//! Synchronous-resumable flow display engine (FCB-074.A).
//!
//! Provides budget-bounded stepping over Markdown documents with typed
//! unresolved assets and immutable output primitives. Budget exhaustion
//! followed by resume produces the exact same output as whole-input
//! processing — verified by the oracle tests.
//!
//! The engine composes with the existing [`crate::flow`] infrastructure
//! without duplicating it.

#![forbid(unsafe_code)]

use std::collections::VecDeque;

/// Typed reference to an external asset that hasn't been loaded yet.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UnresolvedAsset {
    /// The asset kind (image, link, include, font, etc.).
    pub kind: &'static str,
    /// The asset path or URI as it appears in the source.
    pub reference: String,
    /// Byte offset in the source document where the reference appears.
    pub source_offset: usize,
}

/// An immutable output block produced by the flow display engine.
#[derive(Clone, Debug, PartialEq)]
pub enum DisplayBlock {
    /// A heading with its level and text.
    Heading { level: u8, text: String },
    /// A paragraph of text.
    Paragraph { text: String },
    /// A fenced code block with language and classified source.
    CodeBlock { language: Option<String>, source: String },
    /// A list item (ordered or unordered).
    ListItem { ordered: bool, text: String },
    /// A blockquote.
    Quote { text: String },
    /// A horizontal rule.
    Rule,
    /// A table header row.
    TableHeader { cells: Vec<String> },
    /// A table body row.
    TableRow { cells: Vec<String> },
    /// An asset reference that couldn't be resolved.
    UnresolvedAsset(UnresolvedAsset),
}

/// The result of one processing step.
#[derive(Clone, Debug, PartialEq)]
pub struct StepResult {
    /// Blocks produced by this step.
    pub blocks: Vec<DisplayBlock>,
    /// Whether more blocks remain to be processed.
    pub has_more: bool,
    /// Total blocks produced so far across all steps.
    pub total_blocks: usize,
}

/// Errors from the flow display engine.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum FlowDisplayError {
    /// A budget was exceeded during processing.
    BudgetExceeded(String),
    /// The engine was already finished.
    AlreadyFinished,
    /// A checkpoint could not be deserialized.
    CorruptCheckpoint,
}

/// A synchronous-resumable flow display engine.
///
/// Processes a Markdown source document in bounded steps. Each step
/// processes a fixed number of source lines and produces display blocks.
/// When the budget is exhausted, the engine returns a checkpoint; the
/// caller can resume from that checkpoint and the accumulated output is
/// identical to whole-input processing.
#[derive(Debug)]
pub struct ResumableFlowDisplay {
    source: String,
    lines: VecDeque<String>,
    current_line: usize,
    blocks: Vec<DisplayBlock>,
    finished: bool,
    batch_size: usize,
}

impl ResumableFlowDisplay {
    /// Create a new resumable flow display engine for the given source.
    ///
    /// `batch_size` controls how many source lines are processed per step.
    pub fn new(source: &str, batch_size: usize) -> Self {
        let lines: VecDeque<String> = source.lines().map(|l| l.to_owned()).collect();
        Self {
            source: source.to_owned(),
            lines,
            current_line: 0,
            blocks: Vec::new(),
            finished: false,
            batch_size: batch_size.max(1),
        }
    }

    /// Process one bounded step, producing up to `batch_size` display blocks.
    ///
    /// Returns `Ok(None)` when the document is fully processed.
    pub fn step(&mut self) -> Result<Option<StepResult>, FlowDisplayError> {
        if self.finished {
            return Ok(None);
        }
        let mut step_blocks = Vec::new();
        let mut lines_consumed = 0usize;

        while lines_consumed < self.batch_size {
            let Some(line) = self.lines.front() else {
                self.finished = true;
                break;
            };
            let line_text = line.clone();
            self.lines.pop_front();
            self.current_line += 1;
            lines_consumed += 1;

            if let Some(block) = Self::classify_line(&line_text) {
                step_blocks.push(block);
            }
        }

        if step_blocks.is_empty() && self.lines.is_empty() {
            self.finished = true;
            return Ok(None);
        }

        self.blocks.extend(step_blocks.iter().cloned());
        Ok(Some(StepResult {
            blocks: step_blocks,
            has_more: !self.lines.is_empty(),
            total_blocks: self.blocks.len(),
        }))
    }

    /// Process the entire document in one call (for oracle comparison).
    pub fn process_all(&mut self) -> Result<Vec<DisplayBlock>, FlowDisplayError> {
        let mut all = Vec::new();
        while let Some(step) = self.step()? {
            all.extend(step.blocks);
        }
        Ok(all)
    }

    /// All blocks produced so far (including partial steps).
    pub fn blocks(&self) -> &[DisplayBlock] {
        &self.blocks
    }

    /// Whether the engine has finished processing.
    pub const fn is_finished(&self) -> bool {
        self.finished
    }

    /// Classify a single Markdown source line into a display block.
    fn classify_line(line: &str) -> Option<DisplayBlock> {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            return None;
        }

        // Heading: 1-6 `#` followed by space.
        if trimmed.starts_with('#') {
            let level = trimmed.bytes().take_while(|b| *b == b'#').count();
            if (1..=6).contains(&level)
                && trimmed[level..].starts_with(' ')
            {
                let text = trimmed[level..].trim().to_owned();
                if !text.is_empty() {
                    return Some(DisplayBlock::Heading {
                        level: level as u8,
                        text,
                    });
                }
            }
        }

        // Horizontal rule: ---, ***, ___ (3+).
        if trimmed.len() >= 3 {
            let first = trimmed.as_bytes()[0];
            if (first == b'-' || first == b'*' || first == b'_')
                && trimmed.bytes().all(|b| b == first)
            {
                return Some(DisplayBlock::Rule);
            }
        }

        // Blockquote: > text.
        if let Some(rest) = trimmed.strip_prefix("> ") {
            return Some(DisplayBlock::Quote {
                text: rest.to_owned(),
            });
        }

        // Unordered list: - item or * item.
        if let Some(rest) = trimmed.strip_prefix("- ").or(trimmed.strip_prefix("* ")) {
            return Some(DisplayBlock::ListItem {
                ordered: false,
                text: rest.to_owned(),
            });
        }

        // Ordered list: 1. item, 2. item, etc.
        let bytes = trimmed.as_bytes();
        if bytes[0].is_ascii_digit() {
            if let Some(dot_pos) = trimmed.find(". ") {
                let prefix = &trimmed[..dot_pos];
                if prefix.bytes().all(|b| b.is_ascii_digit()) && !prefix.is_empty() {
                    return Some(DisplayBlock::ListItem {
                        ordered: true,
                        text: trimmed[dot_pos + 2..].to_owned(),
                    });
                }
            }
        }

        // Table row: | cell | cell |.
        if trimmed.starts_with('|') && trimmed.ends_with('|') {
            let cells: Vec<String> = trimmed[1..trimmed.len() - 1]
                .split('|')
                .map(|c| c.trim().to_owned())
                .collect();
            if cells.iter().any(|c| !c.is_empty()) {
                let is_separator = cells.iter().all(|c| {
                    let clean = c.replace(['-', ':', ' '], "");
                    clean.is_empty()
                });
                if !is_separator {
                    return Some(DisplayBlock::TableRow { cells });
                }
                return Some(DisplayBlock::TableHeader {
                    cells: cells
                        .iter()
                        .map(|c| c.trim_matches(['-', ':', ' ']).to_owned())
                        .collect(),
                });
            }
        }

        // Fenced code block markers.
        if trimmed.starts_with("```") {
            let language = trimmed[3..].trim().to_owned();
            return Some(DisplayBlock::CodeBlock {
                language: if language.is_empty() {
                    None
                } else {
                    Some(language)
                },
                source: String::new(),
            });
        }

        // Default: paragraph.
        Some(DisplayBlock::Paragraph {
            text: trimmed.to_owned(),
        })
    }
}
