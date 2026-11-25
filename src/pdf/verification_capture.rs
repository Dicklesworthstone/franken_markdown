//! Bounded optional text collection from the pages the writer actually paints.
//!
//! This runs inside page emission, after fitting, contents convergence, final
//! pagination and running-band expansion. It never invokes layout or paging.

use super::{
    PageGeom, Placed, VERIFY_OVERSHOOT_TOLERANCE_PT, VerifyPage, VerifyTextLayer, VerifyTextRun,
    flow_kind_label, line_overshoot, line_text_position, running, seg_display_text,
};
use crate::{RenderError, Result};

const MAX_CAPTURE_BYTES: usize = 8 * 1024 * 1024;
const PAGE_BYTES: usize = 64;
const RUN_BYTES: usize = 192;

#[derive(Default)]
pub(super) struct Capture {
    pages: Vec<VerifyPage>,
    bytes: usize,
}

impl Capture {
    fn admit(&mut self, bytes: usize) -> Result<()> {
        self.bytes = self
            .bytes
            .checked_add(bytes)
            .filter(|&bytes| bytes <= MAX_CAPTURE_BYTES)
            .ok_or_else(limit)?;
        Ok(())
    }

    pub(super) fn page(
        &mut self,
        placed: &[Placed<'_>],
        page_index: usize,
        page: PageGeom,
        chrome: Option<&running::ChromePages>,
    ) -> Result<()> {
        if page_index != self.pages.len() {
            return Err(RenderError::InvalidInput(
                "book_pdf_verification: emitted page order changed".into(),
            ));
        }
        self.admit(PAGE_BYTES)?;
        let mut runs = Vec::new();
        for positioned in placed {
            let line = positioned.line;
            // Admit escaped text and per-run storage before cloning any text.
            // A huge single line cannot allocate a second unbounded copy here.
            self.admit(RUN_BYTES)?;
            let (text, x, overshoot) = if line.rule {
                (String::new(), line.rule_x, None)
            } else if let Some(image) = line.image.as_ref().filter(|image| image.formula) {
                self.admit(escaped_bytes(&image.alt))?;
                (image.alt.clone(), line.rule_x, None)
            } else {
                for segment in &line.segs {
                    self.admit(escaped_bytes(seg_display_text(segment)))?;
                }
                (
                    line.segs.iter().map(seg_display_text).collect(),
                    line.segs.first().map_or(0.0, |segment| segment.x),
                    line_overshoot(line, &page),
                )
            };
            let (size, y) = line_text_position(line, positioned.y);
            runs.push(VerifyTextRun {
                text,
                x,
                y,
                size,
                kind: flow_kind_label(line.flow.kind),
                overshoot,
            });
        }
        if let Some(chrome) = chrome {
            for run in chrome.verification_runs(page_index) {
                self.admit(RUN_BYTES.saturating_add(escaped_bytes(run.text)))?;
                let overshoot = run.x + run.width - page.right_x();
                runs.push(VerifyTextRun {
                    text: run.text.to_string(),
                    x: run.x,
                    y: run.y,
                    size: run.size,
                    kind: run.kind,
                    overshoot: (overshoot > VERIFY_OVERSHOOT_TOLERANCE_PT).then_some(overshoot),
                });
            }
        }
        self.pages.push(VerifyPage {
            number: page_index + 1,
            runs,
        });
        Ok(())
    }

    pub(super) fn finish(self, page_count: usize) -> Result<VerifyTextLayer> {
        if self.pages.len() != page_count {
            return Err(RenderError::InvalidInput(
                "book_pdf_verification: captured and emitted page counts disagree".into(),
            ));
        }
        Ok(VerifyTextLayer {
            page_count,
            pages: self.pages,
        })
    }
}

fn escaped_bytes(text: &str) -> usize {
    text.bytes().fold(0usize, |total, byte| {
        total.saturating_add(match byte {
            b'"' | b'\\' | b'\n' | b'\r' | b'\t' => 2,
            0..=0x1f => 6,
            _ => 1,
        })
    })
}

fn limit() -> RenderError {
    RenderError::InvalidInput(
        "book_pdf_verification_limit: positioned text exceeds the 8 MiB report budget".into(),
    )
}
