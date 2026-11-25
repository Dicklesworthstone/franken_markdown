//! Full verification of the exact chapter-aware PDF publication pipeline.

use super::{BookRenderer, book_pdf_document_with_assets, pdf_links};
use crate::book::LocatedLinkReport;
use crate::{PdfASettings, PdfOptions, PdfParagraphVariantStatus, RenderError, Result};

const MAX_REPORT_BYTES: usize = 8 * 1024 * 1024;

/// One authored reference, retained before temporary PDF action rewriting.
/// Repeated destinations remain separate references in normalized source order.
#[derive(Debug, Clone, PartialEq)]
pub struct BookPdfDestination {
    /// The published chapter containing this source reference.
    pub chapter: String,
    /// Parser-decoded URL or original footnote label, never an internal token.
    pub destination: String,
    /// "link", "footnote", or the generated return action "footnote_backreference".
    pub kind: &'static str,
    /// Original chapter/include source ownership; absence is explicit.
    pub source: crate::book::LinkSourceLocation,
    /// "resolved", "unresolved", or "unchecked" for non-chapter resources.
    pub status: &'static str,
    /// One-based physical page, including generated contents and note-only pages.
    pub page: Option<usize>,
    /// PDF destination coordinate in points, measured from the bottom edge.
    pub y: Option<f32>,
}

/// Bounded layout and navigation verification of one assembled PDF book.
///
/// The text layer, destinations, fitting and pagination come from the same
/// writer used by publication. Source navigation retains chapter/include
/// provenance independently of the temporary URI binding used by that writer.
/// The navigation member keeps its expanded-HTML scope and excludes dormant
/// notes; PDF-specific conclusions are in layout.findings and destinations.
/// This is an authoring/layout report, not PDF/A or PDF/UA certification.
#[derive(Debug, Clone, PartialEq)]
pub struct BookPdfVerificationReport {
    pub layout: crate::verify::VerifyReport,
    pub navigation: LocatedLinkReport,
    pub destinations: Vec<BookPdfDestination>,
    pub pagination: crate::PdfPaginationReport,
}

impl BookPdfVerificationReport {
    /// Serialize the complete deterministic report, never a truncated result.
    ///
    /// # Errors
    /// Refuses a report exceeding 8 MiB of UTF-8 JSON. The writer independently
    /// bounds retained positioned text before this serialization is reached.
    pub fn to_json(&self) -> Result<String> {
        let mut json = Json(String::new());
        json.raw(
            "{\"schema\":\"fmd-book-pdf-verification-v1\",\"scope\":\"assembled-pdf-publication\",\"layout\":",
        )?;
        json.raw(&crate::verify::to_json(&self.layout))?;
        json.raw(",\"navigation\":")?;
        json.raw(&self.navigation.to_json())?;
        json.raw(",\"destinations\":[")?;
        for (index, destination) in self.destinations.iter().enumerate() {
            if index > 0 {
                json.raw(",")?;
            }
            json.raw("{\"chapter\":")?;
            json.string(&destination.chapter)?;
            json.raw(",\"destination\":")?;
            json.string(&destination.destination)?;
            json.raw(",\"kind\":")?;
            json.string(destination.kind)?;
            json.raw(",\"source\":")?;
            json.source(&destination.source)?;
            json.raw(",\"status\":")?;
            json.string(destination.status)?;
            json.raw(",\"page\":")?;
            if let Some(page) = destination.page {
                json.raw(&page.to_string())?;
            } else {
                json.raw("null")?;
            }
            json.raw(",\"y\":")?;
            if let Some(y) = destination.y.filter(|y| y.is_finite()) {
                json.raw(&format!("{y:.3}"))?;
            } else {
                json.raw("null")?;
            }
            json.raw("}")?;
        }
        json.raw("],\"pagination\":{\"requestedOptimal\":")?;
        json.raw(if self.pagination.page_breaks.requested_optimal {
            "true"
        } else {
            "false"
        })?;
        json.raw(",\"strategy\":")?;
        json.string(self.pagination.page_breaks.strategy.as_str())?;
        json.raw(",\"fallback\":")?;
        if let Some(reason) = &self.pagination.page_breaks.fallback {
            json.raw("{\"code\":")?;
            json.string(reason.code())?;
            json.raw(",\"detail\":")?;
            json.string(&reason.to_string())?;
            json.raw("}")?;
        } else {
            json.raw("null")?;
        }
        json.raw(",\"paragraphVariants\":{\"status\":")?;
        let variants = &self.pagination.paragraph_variants;
        json.string(match variants.status {
            PdfParagraphVariantStatus::NotRequested => "not_requested",
            PdfParagraphVariantStatus::UnsupportedTypography => "unsupported_typography",
            PdfParagraphVariantStatus::UnpreparedFootnotes => "unprepared_footnotes",
            PdfParagraphVariantStatus::ContinuedFootnotes => "continued_footnotes",
            PdfParagraphVariantStatus::NoAlternatives => "no_alternatives",
            PdfParagraphVariantStatus::Planned => "planned",
            PdfParagraphVariantStatus::Rejected => "rejected",
        })?;
        for (name, count) in [
            ("attemptedParagraphs", variants.attempted_paragraphs),
            ("limitedParagraphs", variants.limited_paragraphs),
            ("offeredParagraphs", variants.offered_paragraphs),
            ("selectedAlternatives", variants.selected_alternatives),
        ] {
            json.raw(",")?;
            json.string(name)?;
            json.raw(":")?;
            json.raw(&count.to_string())?;
        }
        for (name, limited) in [
            ("paragraphItemLimit", variants.paragraph_item_limit),
            ("retainedLineLimit", variants.retained_line_limit),
            ("measurementWorkLimit", variants.measurement_work_limit),
            ("candidateStateLimit", variants.candidate_state_limit),
        ] {
            json.raw(",")?;
            json.string(name)?;
            json.raw(if limited { ":true" } else { ":false" })?;
        }
        json.raw(",\"failure\":")?;
        if let Some(failure) = &variants.failure {
            json.raw("{\"code\":")?;
            json.string(failure.code())?;
            json.raw(",\"detail\":")?;
            json.string(&failure.to_string())?;
            json.raw("}")?;
        } else {
            json.raw("null")?;
        }
        json.raw("}}}")?;
        Ok(json.0)
    }
}

impl BookRenderer {
    /// Verify the current book with the same retained options as render_pdf.
    /// The book, its assets and its original source ownership remain unchanged.
    ///
    /// # Errors
    /// Propagates publication admission and typed strict-pagination failures,
    /// navigation limits, and the complete 8 MiB report limit.
    pub fn verify_pdf(&self) -> Result<BookPdfVerificationReport> {
        self.verify_pdf_with_options(&self.options().pdf_options(), PdfASettings::OFF)
    }

    /// Verify with borrowed options through the exact book publication plan.
    ///
    /// Chapters retain their language, note and heading scopes. Generated
    /// contents, fitting, continued notes and running page/chapter fields are
    /// captured after final placement. Original missing-link source locations
    /// come from the retained parser capture, never temporary action strings.
    ///
    /// # Errors
    /// Same failures as verify_pdf and render_pdf_with_options.
    pub fn verify_pdf_with_options(
        &self,
        options: &PdfOptions,
        settings: PdfASettings,
    ) -> Result<BookPdfVerificationReport> {
        // Bound source/navigation analysis before loading fonts or rendering.
        let navigation = self.validate_links_with_sources()?;
        let document = book_pdf_document_with_assets(self.book(), &options.image_assets)?;
        let (rendered, destinations) = pdf_links::verify(
            self.book(),
            document.clone(),
            options,
            settings,
            self.source_diagnostics().link_sources(),
        )?;
        let layer = rendered.verification.ok_or_else(|| {
            RenderError::InvalidInput("book_pdf_verification: writer omitted the text layer".into())
        })?;
        let mut layout = crate::verify::report_from_layout(
            &document,
            options,
            layer,
            &rendered.warnings,
            crate::pdf::AnchorAudit {
                resolved: 0,
                unresolved: Vec::new(),
            },
        );
        for chapter in &navigation.report().chapters {
            for finding in &chapter.findings {
                layout.findings.push(crate::verify::VerifyFinding {
                    code: finding.code,
                    detail: format!(
                        "{}: {} ({})",
                        chapter.path, finding.message, finding.destination
                    ),
                });
            }
        }
        for destination in &destinations {
            match destination.status {
                "resolved" => layout.anchors_resolved += 1,
                "unresolved" => {
                    let original = format!("{}: {}", destination.chapter, destination.destination);
                    if !layout.anchors_unresolved.contains(&original) {
                        layout.anchors_unresolved.push(original.clone());
                    }
                    // Preserve all original source findings; add an emitted
                    // destination failure only when source navigation passed.
                    let source_finding = navigation.report().chapters.iter().any(|chapter| {
                        chapter.path == destination.chapter
                            && chapter
                                .findings
                                .iter()
                                .any(|finding| finding.destination == destination.destination)
                    });
                    if !source_finding {
                        layout.findings.push(crate::verify::VerifyFinding {
                            code: "unresolved_pdf_destination",
                            detail: format!("{original} has no emitted PDF destination"),
                        });
                    }
                }
                _ => {}
            }
        }
        crate::verify::refresh_report(&mut layout);
        let report = BookPdfVerificationReport {
            layout,
            navigation,
            destinations,
            pagination: rendered.pagination,
        };
        // Public typed and JSON callers share the same complete-report cap.
        report.to_json()?;
        Ok(report)
    }
}

struct Json(String);
impl Json {
    fn source(&mut self, location: &crate::book::LinkSourceLocation) -> Result<()> {
        use crate::book::{BookSourceLocation, LinkSourcePrecision};
        self.raw("{\"precision\":")?;
        self.string(match location.precision {
            LinkSourcePrecision::Destination => "destination",
            LinkSourcePrecision::Block => "block",
        })?;
        match &location.source {
            BookSourceLocation::Unavailable(reason) => {
                self.raw(",\"status\":\"unavailable\",\"reason\":")?;
                self.string(reason)?;
            }
            BookSourceLocation::Mapped(ranges) => {
                self.raw(",\"status\":\"mapped\",\"ranges\":[")?;
                for (index, range) in ranges.iter().enumerate() {
                    if index > 0 {
                        self.raw(",")?;
                    }
                    self.raw("{\"path\":")?;
                    self.string(&range.path)?;
                    self.raw(",\"start\":")?;
                    self.raw(&range.start.to_string())?;
                    self.raw(",\"end\":")?;
                    self.raw(&range.end.to_string())?;
                    self.raw(",\"kind\":")?;
                    self.string(range.kind.name())?;
                    self.raw("}")?;
                }
                self.raw("]")?;
            }
        }
        self.raw("}")
    }

    fn raw(&mut self, value: &str) -> Result<()> {
        if value.len() > MAX_REPORT_BYTES.saturating_sub(self.0.len()) {
            return Err(RenderError::InvalidInput(
                "book_pdf_verification_limit: JSON report exceeds 8 MiB".into(),
            ));
        }
        self.0.push_str(value);
        Ok(())
    }

    fn string(&mut self, value: &str) -> Result<()> {
        self.raw("\"")?;
        let mut start = 0;
        for (index, byte) in value.bytes().enumerate() {
            let escaped = match byte {
                b'"' => Some("\\\""),
                b'\\' => Some("\\\\"),
                b'\n' => Some("\\n"),
                b'\r' => Some("\\r"),
                b'\t' => Some("\\t"),
                0..=31 => {
                    self.raw(&value[start..index])?;
                    self.raw(&format!("\\u{byte:04x}"))?;
                    start = index + 1;
                    continue;
                }
                _ => None,
            };
            if let Some(escaped) = escaped {
                self.raw(&value[start..index])?;
                self.raw(escaped)?;
                start = index + 1;
            }
        }
        self.raw(&value[start..])?;
        self.raw("\"")
    }
}
