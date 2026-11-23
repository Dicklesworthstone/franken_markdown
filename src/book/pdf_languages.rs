//! Chapter ownership follows canonical transformations of an assembled PDF book.
//! Languages and navigation share that proof, without changing Markdown nodes.

use std::ops::Range;

use crate::book::Book;
use crate::pdf::chapters::Titles;
use crate::pdf::language::{Context, Scopes};
use crate::{Document, PdfOptions, RenderError, Result};

pub(super) struct Plan {
    scopes: Scopes,
    chapters: Titles,
    roots: Vec<u32>,
}

impl Plan {
    pub(super) fn new(
        book: &Book,
        ranges: &[Range<usize>],
        document: &Document,
        options: &PdfOptions,
    ) -> Result<Self> {
        let scopes = Scopes::new(
            book.chapters.iter().map(|chapter| {
                chapter
                    .frontmatter
                    .as_ref()
                    .and_then(|frontmatter| frontmatter.lang.as_deref())
            }),
            options,
        )?;
        let chapters = Titles::new(book.chapters.iter().map(|chapter| {
            if chapter.title.trim().is_empty() {
                chapter.path.as_str()
            } else {
                chapter.title.as_str()
            }
        }))?;
        if ranges.len() != book.chapters.len() {
            return Err(invalid("chapter ranges disagree"));
        }
        let mut roots = vec![0; document.blocks.len()];
        for (chapter, range) in ranges.iter().enumerate() {
            let slice = roots
                .get_mut(range.clone())
                .ok_or_else(|| invalid("invalid chapter range"))?;
            slice.fill(chapter as u32 + 1);
        }
        Ok(Self {
            scopes,
            chapters,
            roots,
        })
    }

    pub(super) fn prepared(
        &self,
        source: &Document,
        profiled: &Document,
        prepared: &Document,
        options: &PdfOptions,
    ) -> Result<Context> {
        if self.roots.len() != source.blocks.len() {
            return Err(invalid("source block ownership changed"));
        }
        // The book has already lowered HTML chapter-by-chapter. The final
        // authoring pass can expand a lowered definition list into paragraphs;
        // use that same canonical transform for ownership, never block text.
        let mut profiled_roots = Vec::with_capacity(profiled.blocks.len());
        for (block, &scope) in source.blocks.iter().zip(&self.roots) {
            let transformed =
                crate::apply_profile_to_blocks(std::slice::from_ref(block), options.profile);
            profiled_roots.extend(std::iter::repeat_n(scope, transformed.len()));
        }
        if profiled_roots.len() != profiled.blocks.len() {
            return Err(invalid("authoring profile changed block ownership"));
        }
        let origins = crate::footnotes::paged_root_origins(profiled)?;
        let roots = origins
            .into_iter()
            .map(|origin| {
                profiled_roots
                    .get(origin)
                    .copied()
                    .ok_or_else(|| invalid("invalid note origin"))
            })
            .collect::<Result<Vec<_>>>()?;
        Context::bind(self.scopes.clone(), self.chapters.clone(), prepared, &roots)
    }
}

fn invalid(message: &str) -> RenderError {
    RenderError::InvalidInput(format!("book_pdf_language: {message}"))
}
