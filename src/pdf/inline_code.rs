//! Source-preserving inline code in PDF paragraphs.
//!
//! Code spaces are painted glyphs, never collapsible or elastic prose glue.
//! Line breaks may occur at source whitespace/separators or, when necessary,
//! between measured source clusters. None of those breaks inserts a hyphen.

use super::{
    BuiltParagraph, EMERGENCY_BREAK_PENALTY, F_MONO, FontSize, LayoutUnit, ParagraphItem,
    PdfWordContext, PdfWordSplitter, Penalty, SEPARATOR_BREAK_PENALTY, SegRunShaper, Tok, TokGroup,
    cjk_break_allowed, is_breakable_whitespace, pdf_separator_break_char,
    push_pdf_word_box_from_vec, text_composition,
};

pub(super) fn push_token(source: &str, strike: bool, link: Option<u32>, out: &mut Vec<Tok>) {
    if !source.is_empty() {
        out.push(Tok {
            text: source.to_owned(),
            slot: F_MONO,
            space: false,
            hard_break: false,
            literal: true,
            link,
            strike,
        });
    }
}

/// Flush a source word containing code. Adjacent prose stays attached to its
/// source spelling; crossing a code boundary must not add a dictionary hyphen.
pub(super) fn flush_word(built: &mut BuiltParagraph, word: &mut Vec<Tok>, cx: PdfWordContext<'_>) {
    let (points, len) = break_points(word, cx);
    let mut splitter = PdfWordSplitter::new(word);
    for (at, penalty) in points {
        let part = splitter.take_to(at);
        if part.is_empty() {
            continue;
        }
        push_pdf_word_box_from_vec(
            built,
            part,
            cx.fs,
            cx.faces,
            cx.width_cache,
            cx.policy.microtype,
        );
        built.items.push(ParagraphItem::Penalty(Penalty {
            width: LayoutUnit::ZERO,
            penalty,
            flagged: false,
        }));
        built.item_toks.push(TokGroup::empty());
        built.break_toks.push(None);
    }
    let tail = splitter.take_to(len);
    push_pdf_word_box_from_vec(
        built,
        tail,
        cx.fs,
        cx.faces,
        cx.width_cache,
        cx.policy.microtype,
    );
    word.clear();
}

/// Find natural breaks and make every intervening source piece fit the real
/// available measure. A fixed character-count threshold is insufficient for
/// large type, host fonts, and deeply indented paragraphs.
fn break_points(word: &[Tok], cx: PdfWordContext<'_>) -> (Vec<(usize, i32)>, usize) {
    let mut clusters = word
        .iter()
        .flat_map(|tok| text_composition::source_clusters(&tok.text).map(move |text| (tok, text)))
        .peekable();
    let mut points = Vec::new();
    let mut cursor = 0;
    let mut chunk_start = 0;
    let mut measured = CodeMeasure::new(cx.faces, cx.fs);
    while let Some((tok, cluster)) = clusters.next() {
        let width = measured.append(tok, cluster);
        if cursor > chunk_start && cx.image_measure.is_some_and(|measure| width > measure) {
            points.push((cursor, EMERGENCY_BREAK_PENALTY));
            measured.clear();
            measured.append(tok, cluster);
            chunk_start = cursor;
        }
        cursor += cluster.chars().count();

        let Some((_, next_cluster)) = clusters.peek() else {
            break;
        };
        let Some(last) = cluster.chars().next_back() else {
            continue;
        };
        let Some(next) = next_cluster.chars().next() else {
            continue;
        };
        let penalty =
            if (tok.literal && is_breakable_whitespace(last)) || cjk_break_allowed(last, next) {
                Some(0)
            } else if pdf_separator_break_char(last) && !pdf_separator_break_char(next) {
                Some(SEPARATOR_BREAK_PENALTY)
            } else {
                None
            };
        if let Some(penalty) = penalty {
            points.push((cursor, penalty));
            measured.clear();
            chunk_start = cursor;
        }
    }
    (points, cursor)
}

struct CodeMeasure<'a> {
    faces: &'a super::Faces,
    size: FontSize,
    complete: LayoutUnit,
    current: Option<CodeRun<'a>>,
}

struct CodeRun<'a> {
    slot: u8,
    link: Option<u32>,
    strike: bool,
    text: String,
    shaper: SegRunShaper<'a>,
}

impl<'a> CodeMeasure<'a> {
    fn new(faces: &'a super::Faces, size: FontSize) -> Self {
        Self {
            faces,
            size,
            complete: LayoutUnit::ZERO,
            current: None,
        }
    }

    fn clear(&mut self) {
        self.complete = LayoutUnit::ZERO;
        self.current = None;
    }

    fn append(&mut self, tok: &Tok, cluster: &str) -> LayoutUnit {
        let same_run = self.current.as_ref().is_some_and(|run| {
            run.slot == tok.slot && run.link == tok.link && run.strike == tok.strike
        });
        if !same_run {
            if let Some(previous) = self.current.take() {
                self.complete += previous.shaper.current_width();
            }
            self.current = Some(CodeRun {
                slot: tok.slot,
                link: tok.link,
                strike: tok.strike,
                text: String::new(),
                shaper: SegRunShaper::new(self.faces.face(tok.slot), self.size),
            });
        }
        if let Some(run) = &mut self.current {
            run.text.push_str(cluster);
            run.shaper.append(cluster, &run.text);
            self.complete + run.shaper.current_width()
        } else {
            self.complete
        }
    }
}
