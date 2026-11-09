//! Chapter-owned language without adding publication metadata to Markdown nodes.
//!
//! Scope identities belong to chapters, even when their interned languages are
//! equal. Prepared block addresses are lookup-only identities within one immutable
//! render; neither addresses nor their ordering enter PDF output.

use std::collections::BTreeMap;
use std::sync::Arc;

use crate::{Block, Document, PdfOptions, RenderError, Result};

/// A newly measured line has not inherited its source block's scope yet.
/// Zero explicitly denotes generated/document-language content.
pub(super) const UNASSIGNED: u32 = u32::MAX;

#[derive(Clone, Default)]
pub(crate) struct Scopes {
    languages: Vec<Scope>,
    document_language: Option<Arc<str>>,
}

#[derive(Clone)]
struct Scope {
    language: Arc<str>,
    cache_id: u32,
}

impl Scopes {
    pub(crate) fn new<'a>(
        chapters: impl Iterator<Item = Option<&'a str>>,
        options: &PdfOptions,
    ) -> Result<Self> {
        let chapters: Vec<_> = chapters
            .map(|lang| lang.map(str::trim).filter(|lang| !lang.is_empty()))
            .collect();
        if chapters.iter().all(Option::is_none) {
            return Ok(Self::default());
        }
        if chapters.len() >= UNASSIGNED as usize {
            return Err(RenderError::InvalidInput(
                "book_pdf_language: too many chapter scopes".to_string(),
            ));
        }
        let fallback = options
            .lang
            .as_deref()
            .map(str::trim)
            .filter(|lang| !lang.is_empty())
            .unwrap_or("en-US");
        let mut interned = BTreeMap::<&str, Scope>::new();
        let languages = chapters
            .into_iter()
            .map(|lang| {
                let lang = lang.unwrap_or(fallback);
                let cache_id = interned.len() as u32 + 1;
                interned
                    .entry(lang)
                    .or_insert_with(|| Scope {
                        language: Arc::from(lang),
                        cache_id,
                    })
                    .clone()
            })
            .collect();
        let document_language = Some(
            interned
                .get(fallback)
                .map_or_else(|| Arc::from(fallback), |scope| Arc::clone(&scope.language)),
        );
        Ok(Self {
            languages,
            document_language,
        })
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.languages.is_empty()
    }

    pub(super) fn language(&self, scope: u32) -> Option<&Arc<str>> {
        scope
            .checked_sub(1)
            .and_then(|index| self.languages.get(index as usize))
            .map(|scope| &scope.language)
    }

    pub(super) fn document_language(&self) -> Option<&Arc<str>> {
        self.document_language.as_ref()
    }

    fn cache_id(&self, scope: u32) -> u32 {
        scope
            .checked_sub(1)
            .and_then(|index| self.languages.get(index as usize))
            .map_or(0, |scope| scope.cache_id)
    }
}

pub(crate) struct Context {
    pub(super) scopes: Scopes,
    blocks: BTreeMap<usize, u32>,
}

pub(super) static EMPTY: Context = Context {
    scopes: Scopes {
        languages: Vec::new(),
        document_language: None,
    },
    blocks: BTreeMap::new(),
};

impl Context {
    pub(crate) fn bind(scopes: Scopes, document: &Document, roots: &[u32]) -> Result<Self> {
        let mut context = Self {
            scopes,
            blocks: BTreeMap::new(),
        };
        if context.scopes.is_empty() {
            return Ok(context);
        }
        if roots.len() != document.blocks.len() {
            return Err(RenderError::InvalidInput(
                "book_pdf_language: prepared block ownership differs from the document".to_string(),
            ));
        }
        for (block, &scope) in document.blocks.iter().zip(roots) {
            if scope != 0 && context.scopes.language(scope).is_none() {
                return Err(RenderError::InvalidInput(
                    "book_pdf_language: invalid prepared chapter scope".to_string(),
                ));
            }
            context
                .blocks
                .insert(std::ptr::from_ref(block) as usize, scope);
            if let Block::FootnoteDefinition { blocks, .. } = block {
                // Notes are laid out from their body slice after normal body
                // flow. Bind those roots too; nested blocks inherit normally.
                for child in blocks {
                    context
                        .blocks
                        .insert(std::ptr::from_ref(child) as usize, scope);
                }
            }
        }
        Ok(context)
    }

    pub(super) fn cache_id(&self, scope: u32) -> u32 {
        self.scopes.cache_id(scope)
    }

    pub(super) fn scope(&self, block: &Block) -> Option<u32> {
        self.blocks
            .get(&(std::ptr::from_ref(block) as usize))
            .copied()
    }

    pub(super) fn language<'a>(&'a self, scope: u32, options: &'a PdfOptions) -> Option<&'a str> {
        self.scopes
            .language(scope)
            .map(AsRef::as_ref)
            .or(options.lang.as_deref())
    }
}
