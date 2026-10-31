//! Provenance for the page planner actually used by PDF emission.
//!
//! Exactness is scoped to measured lines and admitted paragraph variants.
//! It does not claim that every possible line layout was searched.

use crate::pagination::height::HeightPaginationError;
use std::fmt;

/// Page-boundary algorithm that produced the emitted PDF.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PdfPaginationStrategy {
    /// Local page filling, also used when footnote-aware exact planning fails.
    Greedy,
    /// The bounded, fixed-point planner satisfied its hard block constraints.
    ExactHeight,
    /// Guaranteed-progress page planning with soft keep/widow/orphan costs.
    Legacy,
}

impl PdfPaginationStrategy {
    /// Stable selector for host diagnostics.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Greedy => "greedy",
            Self::ExactHeight => "exact_height",
            Self::Legacy => "legacy",
        }
    }
}

/// Why an exact PDF pagination request could not be honored.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PdfPaginationFailure {
    /// The original typed solver error, including its budget or block index.
    Planner(HeightPaginationError),
    /// A complete solver result could not be mapped safely to emitted lines.
    InvalidPlan(&'static str),
}

impl PdfPaginationFailure {
    /// Stable machine-readable reason; details remain in the typed variant.
    #[must_use]
    pub const fn code(&self) -> &'static str {
        match self {
            Self::Planner(HeightPaginationError::BudgetExceeded(_)) => "budget_exceeded",
            Self::Planner(HeightPaginationError::NoFeasibleLayout { .. }) => "infeasible",
            Self::Planner(HeightPaginationError::InvalidOptions(_))
            | Self::Planner(HeightPaginationError::InvalidBlock { .. }) => "invalid_input",
            Self::Planner(HeightPaginationError::CostOverflow) => "cost_overflow",
            Self::InvalidPlan(_) => "invalid_plan",
        }
    }

    /// Shared stable selector for best-effort warnings and strict refusals.
    pub(crate) const fn diagnostic_code(&self) -> &'static str {
        match self {
            Self::Planner(HeightPaginationError::BudgetExceeded(_)) => "pagination_budget_exceeded",
            Self::Planner(HeightPaginationError::NoFeasibleLayout { .. }) => {
                "pagination_infeasible"
            }
            Self::Planner(HeightPaginationError::InvalidOptions(_))
            | Self::Planner(HeightPaginationError::InvalidBlock { .. }) => {
                "pagination_invalid_input"
            }
            Self::Planner(HeightPaginationError::CostOverflow) => "pagination_cost_overflow",
            Self::InvalidPlan(_) => "pagination_invalid_plan",
        }
    }
}

impl fmt::Display for PdfPaginationFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Planner(error) => error.fmt(f),
            Self::InvalidPlan(reason) => write!(f, "PDF pagination plan was rejected: {reason}"),
        }
    }
}

impl std::error::Error for PdfPaginationFailure {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Planner(error) => Some(error),
            Self::InvalidPlan(_) => None,
        }
    }
}

/// Actual page-boundary planning for the final measured lines.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PdfPageBreakReport {
    pub requested_optimal: bool,
    pub strategy: PdfPaginationStrategy,
    /// `Some` only when an optimal request fell back to a weaker algorithm.
    pub fallback: Option<PdfPaginationFailure>,
}

impl PdfPageBreakReport {
    /// Whether the emitted page boundaries use the exact fixed-layout planner.
    /// Paragraph-variant admission is reported separately.
    #[must_use]
    pub const fn is_exact(&self) -> bool {
        matches!(self.strategy, PdfPaginationStrategy::ExactHeight) && self.fallback.is_none()
    }
}

/// Result of the optional same-measure paragraph-variant search.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum PdfParagraphVariantStatus {
    /// `optimal_pagination` is disabled.
    #[default]
    NotRequested,
    /// Gradual, river, or Pareto typography retains its original line layouts.
    UnsupportedTypography,
    /// The low-level document contains footnotes not prepared for page layout.
    UnpreparedFootnotes,
    /// No usable alternatives were offered by the bounded candidate generator.
    NoAlternatives,
    /// Exact joint selection completed over the admitted measured variants.
    Planned,
    /// Joint selection failed; the complete original measured layout was kept.
    Rejected,
}

/// Admission and result of paragraph alternatives in the final layout pass.
///
/// Counts exclude fit-to-pages probes and earlier TOC passes. `Planned` means
/// exact selection among offered variants, not all possible paragraph layouts.
/// Unsupported flows (such as table cells and display-equation blocks) retain
/// their ordinary measured shape by design.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PdfParagraphVariantReport {
    pub status: PdfParagraphVariantStatus,
    /// Paragraphs admitted to the adjacent-line-count candidate generator.
    pub attempted_paragraphs: usize,
    /// Paragraphs whose candidate generation hit an explicit resource limit.
    pub limited_paragraphs: usize,
    pub paragraph_item_limit: bool,
    pub retained_line_limit: bool,
    pub measurement_work_limit: bool,
    pub candidate_state_limit: bool,
    /// Blocks with at least one eligible alternative offered to joint selection.
    pub offered_paragraphs: usize,
    /// Offered blocks whose alternative was actually installed in the output.
    pub selected_alternatives: usize,
    pub failure: Option<PdfPaginationFailure>,
}

impl PdfParagraphVariantReport {
    /// Whether a mode restriction, budget, or failure retained baseline shapes.
    /// Ordinary absence of alternatives is not a warning.
    #[must_use]
    pub fn is_limited(&self) -> bool {
        self.limited_paragraphs > 0
            || self.failure.is_some()
            || matches!(
                self.status,
                PdfParagraphVariantStatus::UnsupportedTypography
                    | PdfParagraphVariantStatus::UnpreparedFootnotes
            )
    }

    pub(super) fn limitation_message(&self) -> String {
        if let Some(failure) = &self.failure {
            return format!(
                "joint paragraph selection failed ({failure}); retained the original measured layout"
            );
        }
        match self.status {
            PdfParagraphVariantStatus::UnsupportedTypography =>
                "paragraph alternatives are unavailable with gradual, river, or Pareto line breaking; retained those measured paragraph shapes".to_string(),
            PdfParagraphVariantStatus::UnpreparedFootnotes =>
                "paragraph alternatives require prepared numbered footnotes; retained the original measured paragraph shapes".to_string(),
            _ => {
                let mut limits = Vec::new();
                if self.paragraph_item_limit { limits.push("paragraph items"); }
                if self.retained_line_limit { limits.push("retained alternative lines"); }
                if self.measurement_work_limit { limits.push("measurement work"); }
                if self.candidate_state_limit { limits.push("candidate state cells"); }
                format!("paragraph alternatives reached the {} limit for {} paragraph(s); retained their original measured shapes", limits.join(", "), self.limited_paragraphs)
            }
        }
    }
}

/// Pagination provenance from the same layout and emission as the PDF bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PdfPaginationReport {
    pub page_breaks: PdfPageBreakReport,
    pub paragraph_variants: PdfParagraphVariantReport,
}

impl PdfPaginationReport {
    pub(super) fn append_warnings(&self, warnings: &mut Vec<super::RenderWarning>) {
        if let Some(reason) = &self.page_breaks.fallback {
            warnings.push(super::RenderWarning::PaginationFallback {
                strategy: self.page_breaks.strategy,
                reason: reason.clone(),
            });
        }
        if self.paragraph_variants.is_limited() {
            warnings.push(super::RenderWarning::ParagraphPaginationLimited(
                self.paragraph_variants.clone(),
            ));
        }
    }
}

/// PDF bytes and their pagination provenance, without timing instrumentation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PdfRenderReport {
    pub bytes: Vec<u8>,
    pub pagination: PdfPaginationReport,
}
