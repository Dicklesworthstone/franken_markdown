//! One immutable authoring grammar for every source and provenance probe in a
//! language-server session. Profiles must be selected before parsing; rewriting
//! an already parsed definition list cannot recover cross-line inline syntax.

use franken_markdown::parse::{DestinationSpan, ReferenceLabelSpan};
use franken_markdown::{Document, Profile, SpannedDocument};

use super::Json;

#[derive(Clone, Copy, Debug, Default)]
pub(super) struct Parser {
    profile: Option<Profile>,
}

impl Parser {
    /// Omitted/null initialization options preserve the historical extended
    /// grammar. Explicit profiles use the same strict tokens as MCP and Flow.
    pub fn initialize(params: &Json) -> Result<Self, &'static str> {
        let options = match params.get("initializationOptions") {
            None | Some(Json::Null) => return Ok(Self::default()),
            Some(options) => options
                .as_object()
                .ok_or("initializationOptions must be an object or null")?,
        };
        let profile = match options.get("profile") {
            None => None,
            Some(Json::String(value)) if value == "commonmark-gfm" => Some(Profile::CommonMarkGfm),
            Some(Json::String(value)) if value == "gfm-plus" => Some(Profile::GfmPlus),
            Some(_) => {
                return Err("initializationOptions.profile must be 'commonmark-gfm' or 'gfm-plus'");
            }
        };
        Ok(Self { profile })
    }

    pub fn parse(self, source: &str) -> Document {
        franken_markdown::parse_markdown_with_profile(source, self.profile)
    }

    pub fn spanned(self, source: &str) -> SpannedDocument {
        franken_markdown::parse_markdown_spanned_with_profile(source, self.profile)
    }

    pub fn destinations(self, source: &str) -> Vec<DestinationSpan> {
        franken_markdown::parse::destination_spans_with_profile(source, self.profile)
    }

    pub fn reference_labels(self, source: &str) -> Vec<ReferenceLabelSpan> {
        franken_markdown::parse::reference_label_spans_with_profile(source, self.profile)
    }

    pub fn profile(self) -> Option<Profile> {
        self.profile
    }
}
