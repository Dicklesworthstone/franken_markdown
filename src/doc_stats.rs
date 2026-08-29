//! Document intelligence, readability scoring, and structural linting for Markdown AST.
//!
//! Provides word counting, reading/speaking time estimates, Flesch Reading Ease,
//! Flesch-Kincaid Grade Level, structural hierarchy analysis, outline extraction,
//! and linter findings (broken internal anchors, heading hierarchy skips, etc.).

use std::collections::{BTreeMap, BTreeSet};

use crate::ast::{Block, Document, Inline, ListItem, Table};

/// Aggregated document telemetry, readability scores, and health analysis.
#[derive(Debug, Clone, PartialEq)]
pub struct DocumentStats {
    /// Total input byte size.
    pub bytes: usize,
    /// Total line count.
    pub lines: usize,
    /// Total words in body text, headings, tables, and lists.
    pub words: usize,
    /// Total characters (excluding whitespace).
    pub characters: usize,
    /// Total sentence count.
    pub sentences: usize,
    /// Estimated total syllable count.
    pub syllables: usize,
    /// Estimated silent reading time in seconds (standard 220 WPM).
    pub reading_time_secs: u32,
    /// Estimated speaking / presentation time in seconds (standard 130 WPM).
    pub speaking_time_secs: u32,
    /// Flesch Reading Ease score (0–100+, higher = easier to read).
    pub flesch_reading_ease: f32,
    /// Flesch-Kincaid Grade Level (e.g. 8.0 = 8th grade reading level).
    pub flesch_kincaid_grade: f32,
    /// Readability tier label ("Very Easy", "Standard", "Difficult", etc.).
    pub reading_ease_label: &'static str,
    /// Structural element breakdown.
    pub structure: DocumentStructure,
    /// Document outline (headings in order).
    pub outline: Vec<OutlineHeading>,
    /// Quality & integrity findings.
    pub findings: Vec<DocFinding>,
}

/// Structural inventory of document elements.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct DocumentStructure {
    pub headings_total: usize,
    pub headings_by_level: [usize; 6],
    pub paragraphs: usize,
    pub code_blocks: usize,
    pub code_languages: Vec<String>,
    pub tables: usize,
    pub table_rows: usize,
    pub table_cells: usize,
    pub lists: usize,
    pub list_items: usize,
    pub task_items_total: usize,
    pub task_items_completed: usize,
    pub blockquotes: usize,
    pub callouts_total: usize,
    pub callouts_by_kind: BTreeMap<String, usize>,
    pub math_blocks: usize,
    pub math_inlines: usize,
    pub links_total: usize,
    pub links_external: usize,
    pub links_internal_anchors: usize,
    pub images: usize,
    pub footnote_definitions: usize,
    pub footnote_references: usize,
}

/// A heading entry in the document outline.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OutlineHeading {
    pub level: u8,
    pub text: String,
    pub slug: String,
}

/// A document health or structure finding.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DocFinding {
    pub severity: &'static str,
    pub code: &'static str,
    pub message: String,
}

/// Analyze a Markdown source string and its parsed AST to produce document stats.
#[must_use]
pub fn compute_doc_stats(markdown: &str, doc: &Document) -> DocumentStats {
    let bytes = markdown.len();
    let lines = if markdown.is_empty() {
        0
    } else {
        markdown.lines().count()
    };

    let mut words = 0;
    let mut characters = 0;
    let mut sentences = 0;
    let mut syllables = 0;

    let mut structure = DocumentStructure::default();
    let mut outline = Vec::new();
    let mut code_langs_set = BTreeSet::new();

    let mut defined_anchors: BTreeSet<String> = BTreeSet::new();
    let mut slug_counts: BTreeMap<String, usize> = BTreeMap::new();
    let mut referenced_anchors: Vec<(String, String)> = Vec::new(); // (slug, link_text)
    let mut footnote_defs_set = BTreeSet::new();
    let mut footnote_refs_set = BTreeSet::new();

    let mut last_heading_level: Option<u8> = None;
    let mut findings = Vec::new();

    walk_blocks_for_stats(
        &doc.blocks,
        &mut words,
        &mut characters,
        &mut sentences,
        &mut syllables,
        &mut structure,
        &mut outline,
        &mut code_langs_set,
        &mut defined_anchors,
        &mut slug_counts,
        &mut referenced_anchors,
        &mut footnote_defs_set,
        &mut footnote_refs_set,
        &mut last_heading_level,
        &mut findings,
    );

    structure.code_languages = code_langs_set.into_iter().collect();

    // Check for broken internal anchors (e.g. [Link](#missing-target))
    for (anchor, link_text) in &referenced_anchors {
        if !defined_anchors.contains(anchor) && !footnote_defs_set.contains(anchor) {
            findings.push(DocFinding {
                severity: "warning",
                code: "broken_internal_anchor",
                message: format!(
                    "link '{}' targets internal anchor '#{}' which does not exist in the document",
                    link_text, anchor
                ),
            });
        }
    }

    // Check for undefined footnote references
    for fref in &footnote_refs_set {
        if !footnote_defs_set.contains(fref) {
            findings.push(DocFinding {
                severity: "warning",
                code: "undefined_footnote",
                message: format!("footnote reference '[^{}]' has no matching definition", fref),
            });
        }
    }

    // Check for unreferenced footnote definitions
    for fdef in &footnote_defs_set {
        if !footnote_refs_set.contains(fdef) {
            findings.push(DocFinding {
                severity: "info",
                code: "unreferenced_footnote_def",
                message: format!("footnote definition '[^{}]:' is never referenced", fdef),
            });
        }
    }

    // Sentence count sanity floor
    let effective_sentences = sentences.max(if words > 0 { 1 } else { 0 });
    let effective_words = words.max(if effective_sentences > 0 { 1 } else { 0 });
    let effective_syllables = syllables.max(effective_words);

    let reading_time_secs = ((words as f32) / (220.0 / 60.0)).round() as u32;
    let speaking_time_secs = ((words as f32) / (130.0 / 60.0)).round() as u32;

    let (flesch_reading_ease, flesch_kincaid_grade, reading_ease_label) = if words == 0 {
        (100.0, 0.0, "N/A")
    } else {
        let w_per_s = (words as f32) / (effective_sentences as f32);
        let syl_per_w = (effective_syllables as f32) / (words as f32);

        let fre = 206.835 - (1.015 * w_per_s) - (84.6 * syl_per_w);
        let fkg = (0.39 * w_per_s) + (11.8 * syl_per_w) - 15.59;

        let label = if fre >= 90.0 {
            "Very Easy (5th grade)"
        } else if fre >= 80.0 {
            "Easy (6th grade)"
        } else if fre >= 70.0 {
            "Fairly Easy (7th grade)"
        } else if fre >= 60.0 {
            "Standard (8th–9th grade)"
        } else if fre >= 50.0 {
            "Fairly Difficult (10th–12th grade)"
        } else if fre >= 30.0 {
            "Difficult (College)"
        } else {
            "Very Confusing (Graduate)"
        };

        (fre.clamp(0.0, 100.0), fkg.max(0.0), label)
    };

    DocumentStats {
        bytes,
        lines,
        words,
        characters,
        sentences: effective_sentences,
        syllables: effective_syllables,
        reading_time_secs,
        speaking_time_secs,
        flesch_reading_ease,
        flesch_kincaid_grade,
        reading_ease_label,
        structure,
        outline,
        findings,
    }
}

fn walk_blocks_for_stats(
    blocks: &[Block],
    words: &mut usize,
    characters: &mut usize,
    sentences: &mut usize,
    syllables: &mut usize,
    structure: &mut DocumentStructure,
    outline: &mut Vec<OutlineHeading>,
    code_langs_set: &mut BTreeSet<String>,
    defined_anchors: &mut BTreeSet<String>,
    slug_counts: &mut BTreeMap<String, usize>,
    referenced_anchors: &mut Vec<(String, String)>,
    footnote_defs_set: &mut BTreeSet<String>,
    footnote_refs_set: &mut BTreeSet<String>,
    last_heading_level: &mut Option<u8>,
    findings: &mut Vec<DocFinding>,
) {
    for block in blocks {
        match block {
            Block::Heading { level, inlines } => {
                let lvl = (*level as usize).clamp(1, 6);
                structure.headings_total += 1;
                structure.headings_by_level[lvl - 1] += 1;

                if let Some(prev) = *last_heading_level {
                    if *level > prev + 1 {
                        findings.push(DocFinding {
                            severity: "warning",
                            code: "heading_hierarchy_skip",
                            message: format!(
                                "heading level H{} follows H{} (skipped H{})",
                                level,
                                prev,
                                prev + 1
                            ),
                        });
                    }
                }
                *last_heading_level = Some(*level);

                let plain_text = inlines_to_plain(inlines);
                if plain_text.trim().is_empty() {
                    findings.push(DocFinding {
                        severity: "warning",
                        code: "empty_heading",
                        message: format!("heading at level H{} has no text content", level),
                    });
                }

                let base_slug = slug_from_text(&plain_text);
                let full_slug = if let Some(count) = slug_counts.get_mut(&base_slug) {
                    *count += 1;
                    let suffixed = format!("{}-{}", base_slug, *count - 1);
                    findings.push(DocFinding {
                        severity: "info",
                        code: "duplicate_heading_slug",
                        message: format!(
                            "duplicate heading text '{}' generated collision anchor '#{}'",
                            plain_text, suffixed
                        ),
                    });
                    suffixed
                } else {
                    slug_counts.insert(base_slug.clone(), 1);
                    base_slug
                };

                defined_anchors.insert(full_slug.clone());
                outline.push(OutlineHeading {
                    level: *level,
                    text: plain_text,
                    slug: full_slug,
                });

                analyze_inlines(
                    inlines,
                    words,
                    characters,
                    sentences,
                    syllables,
                    structure,
                    referenced_anchors,
                    footnote_refs_set,
                );
                // Headings are typically sentences/standalone thoughts
                *sentences += 1;
            }
            Block::Paragraph(inlines) => {
                structure.paragraphs += 1;
                analyze_inlines(
                    inlines,
                    words,
                    characters,
                    sentences,
                    syllables,
                    structure,
                    referenced_anchors,
                    footnote_refs_set,
                );
            }
            Block::CodeBlock { lang, code } => {
                structure.code_blocks += 1;
                if let Some(l) = lang {
                    let cleaned = l.trim().to_ascii_lowercase();
                    if !cleaned.is_empty() {
                        code_langs_set.insert(cleaned);
                    }
                }
                *characters += code.chars().filter(|c| !c.is_whitespace()).count();
            }
            Block::BlockQuote(inner) => {
                if let Some((tag, _label, body)) = crate::ast::alert_body(inner) {
                    structure.callouts_total += 1;
                    *structure
                        .callouts_by_kind
                        .entry(tag.to_string())
                        .or_insert(0) += 1;
                    walk_blocks_for_stats(
                        &body,
                        words,
                        characters,
                        sentences,
                        syllables,
                        structure,
                        outline,
                        code_langs_set,
                        defined_anchors,
                        slug_counts,
                        referenced_anchors,
                        footnote_defs_set,
                        footnote_refs_set,
                        last_heading_level,
                        findings,
                    );
                } else {
                    structure.blockquotes += 1;
                    walk_blocks_for_stats(
                        inner,
                        words,
                        characters,
                        sentences,
                        syllables,
                        structure,
                        outline,
                        code_langs_set,
                        defined_anchors,
                        slug_counts,
                        referenced_anchors,
                        footnote_defs_set,
                        footnote_refs_set,
                        last_heading_level,
                        findings,
                    );
                }
            }
            Block::List(list) => {
                structure.lists += 1;
                for item in &list.items {
                    structure.list_items += 1;
                    if let Some(checked) = item.task {
                        structure.task_items_total += 1;
                        if checked {
                            structure.task_items_completed += 1;
                        }
                    }
                    walk_blocks_for_stats(
                        &item.blocks,
                        words,
                        characters,
                        sentences,
                        syllables,
                        structure,
                        outline,
                        code_langs_set,
                        defined_anchors,
                        slug_counts,
                        referenced_anchors,
                        footnote_defs_set,
                        footnote_refs_set,
                        last_heading_level,
                        findings,
                    );
                }
            }
            Block::Table(table) => {
                structure.tables += 1;
                structure.table_rows += table.rows.len() + 1; // head + body rows
                for cell in &table.head {
                    structure.table_cells += 1;
                    analyze_inlines(
                        cell,
                        words,
                        characters,
                        sentences,
                        syllables,
                        structure,
                        referenced_anchors,
                        footnote_refs_set,
                    );
                }
                for row in &table.rows {
                    for cell in row {
                        structure.table_cells += 1;
                        analyze_inlines(
                            cell,
                            words,
                            characters,
                            sentences,
                            syllables,
                            structure,
                            referenced_anchors,
                            footnote_refs_set,
                        );
                    }
                }
            }
            Block::ThematicBreak => {}
            Block::HtmlBlock(html) => {
                *characters += html.chars().filter(|c| !c.is_whitespace()).count();
            }
            Block::FootnoteDefinition { id, blocks } => {
                structure.footnote_definitions += 1;
                footnote_defs_set.insert(id.clone());
                walk_blocks_for_stats(
                    blocks,
                    words,
                    characters,
                    sentences,
                    syllables,
                    structure,
                    outline,
                    code_langs_set,
                    defined_anchors,
                    slug_counts,
                    referenced_anchors,
                    footnote_defs_set,
                    footnote_refs_set,
                    last_heading_level,
                    findings,
                );
            }
            Block::MathBlock(math) => {
                structure.math_blocks += 1;
                *characters += math.chars().filter(|c| !c.is_whitespace()).count();
            }
            Block::DefinitionList(items) => {
                for item in items {
                    for term in &item.terms {
                        analyze_inlines(
                            term,
                            words,
                            characters,
                            sentences,
                            syllables,
                            structure,
                            referenced_anchors,
                            footnote_refs_set,
                        );
                    }
                    for def in &item.definitions {
                        analyze_inlines(
                            def,
                            words,
                            characters,
                            sentences,
                            syllables,
                            structure,
                            referenced_anchors,
                            footnote_refs_set,
                        );
                    }
                }
            }
        }
    }
}

fn analyze_inlines(
    inlines: &[Inline],
    words: &mut usize,
    characters: &mut usize,
    sentences: &mut usize,
    syllables: &mut usize,
    structure: &mut DocumentStructure,
    referenced_anchors: &mut Vec<(String, String)>,
    footnote_refs_set: &mut BTreeSet<String>,
) {
    for inline in inlines {
        match inline {
            Inline::Text(t) => {
                process_prose_text(t, words, characters, sentences, syllables);
            }
            Inline::Emphasis(inner) | Inline::Strong(inner) | Inline::Strikethrough(inner) => {
                analyze_inlines(
                    inner,
                    words,
                    characters,
                    sentences,
                    syllables,
                    structure,
                    referenced_anchors,
                    footnote_refs_set,
                );
            }
            Inline::Code(code) => {
                *characters += code.chars().filter(|c| !c.is_whitespace()).count();
                process_prose_text(code, words, characters, sentences, syllables);
            }
            Inline::Link {
                dest,
                content,
                ..
            } => {
                structure.links_total += 1;
                if let Some(anchor) = dest.strip_prefix('#') {
                    structure.links_internal_anchors += 1;
                    let link_text = inlines_to_plain(content);
                    referenced_anchors.push((anchor.to_string(), link_text));
                } else {
                    structure.links_external += 1;
                }
                analyze_inlines(
                    content,
                    words,
                    characters,
                    sentences,
                    syllables,
                    structure,
                    referenced_anchors,
                    footnote_refs_set,
                );
            }
            Inline::Image { alt, .. } => {
                structure.images += 1;
                process_prose_text(alt, words, characters, sentences, syllables);
            }
            Inline::SoftBreak | Inline::HardBreak => {}
            Inline::Html(h) => {
                *characters += h.chars().filter(|c| !c.is_whitespace()).count();
            }
            Inline::FootnoteRef { id } => {
                structure.footnote_references += 1;
                footnote_refs_set.insert(id.clone());
            }
            Inline::Math(m) | Inline::DisplayMath(m) => {
                structure.math_inlines += 1;
                *characters += m.chars().filter(|c| !c.is_whitespace()).count();
            }
        }
    }
}

fn process_prose_text(
    text: &str,
    words: &mut usize,
    characters: &mut usize,
    sentences: &mut usize,
    syllables: &mut usize,
) {
    for ch in text.chars() {
        if !ch.is_whitespace() {
            *characters += 1;
        }
    }

    for word in text.split_whitespace() {
        let trimmed: String = word
            .chars()
            .filter(|c| c.is_alphanumeric() || *c == '-' || *c == '\'')
            .collect();
        if !trimmed.is_empty() {
            *words += 1;
            *syllables += count_syllables_in_word(&trimmed);
        }
    }

    // Sentence detection based on terminal punctuation
    let bytes = text.as_bytes();
    for (i, &b) in bytes.iter().enumerate() {
        if b == b'.' || b == b'?' || b == b'!' {
            if i + 1 == bytes.len() || bytes[i + 1].is_ascii_whitespace() {
                *sentences += 1;
            }
        }
    }
}

fn count_syllables_in_word(word: &str) -> usize {
    let lower = word.to_ascii_lowercase();
    let cleaned: String = lower.chars().filter(|c| c.is_ascii_alphabetic()).collect();
    if cleaned.is_empty() {
        return 0;
    }
    let bytes = cleaned.as_bytes();
    let len = bytes.len();
    if len <= 3 {
        return 1;
    }
    let is_vowel = |b: u8| matches!(b, b'a' | b'e' | b'i' | b'o' | b'u' | b'y');
    let mut count = 0;
    let mut prev_vowel = false;
    for &b in bytes {
        if is_vowel(b) {
            if !prev_vowel {
                count += 1;
            }
            prev_vowel = true;
        } else {
            prev_vowel = false;
        }
    }
    // Adjust for silent trailing 'e'
    if bytes.ends_with(b"e") && !bytes.ends_with(b"le") && count > 1 && !is_vowel(bytes[len - 2]) {
        count -= 1;
    }
    // Adjust for "-ed" or "-es" endings
    if (bytes.ends_with(b"ed") || bytes.ends_with(b"es"))
        && count > 1
        && len > 4
        && !matches!(bytes[len - 3], b't' | b'd' | b's' | b'z' | b'c' | b'g' | b'j')
    {
        count -= 1;
    }
    count.max(1)
}

fn inlines_to_plain(inlines: &[Inline]) -> String {
    let mut s = String::new();
    for inl in inlines {
        match inl {
            Inline::Text(t) | Inline::Code(t) | Inline::Html(t) | Inline::Math(t) | Inline::DisplayMath(t) => s.push_str(t),
            Inline::Emphasis(c) | Inline::Strong(c) | Inline::Strikethrough(c) => s.push_str(&inlines_to_plain(c)),
            Inline::Link { content, .. } => s.push_str(&inlines_to_plain(content)),
            Inline::Image { alt, .. } => s.push_str(alt),
            Inline::SoftBreak | Inline::HardBreak => s.push(' '),
            Inline::FootnoteRef { .. } => {}
        }
    }
    s
}

fn slug_from_text(text: &str) -> String {
    let mut s = String::new();
    let mut pending_dash = false;
    for c in text.chars() {
        if c.is_ascii_alphanumeric() {
            if pending_dash && !s.is_empty() {
                s.push('-');
            }
            s.push(c.to_ascii_lowercase());
            pending_dash = false;
        } else if c == ' ' || c == '-' || c == '_' {
            pending_dash = true;
        }
    }
    s
}

impl DocumentStats {
    /// Emit machine-readable JSON representation.
    #[must_use]
    pub fn to_json(&self) -> String {
        let mut out = String::with_capacity(4096);
        out.push('{');
        out.push_str("\"schema\":\"fmd-document-stats-v1\",");
        out.push_str(&format!("\"bytes\":{},", self.bytes));
        out.push_str(&format!("\"lines\":{},", self.lines));
        out.push_str(&format!("\"words\":{},", self.words));
        out.push_str(&format!("\"characters\":{},", self.characters));
        out.push_str(&format!("\"sentences\":{},", self.sentences));
        out.push_str(&format!("\"syllables\":{},", self.syllables));
        out.push_str(&format!("\"reading_time_secs\":{},", self.reading_time_secs));
        out.push_str(&format!("\"speaking_time_secs\":{},", self.speaking_time_secs));
        out.push_str(&format!("\"flesch_reading_ease\":{:.2},", self.flesch_reading_ease));
        out.push_str(&format!("\"flesch_kincaid_grade\":{:.2},", self.flesch_kincaid_grade));
        out.push_str(&format!("\"reading_ease_label\":\"{}\",", self.reading_ease_label));

        // Structure
        out.push_str("\"structure\":{");
        out.push_str(&format!("\"headings_total\":{},", self.structure.headings_total));
        out.push_str(&format!(
            "\"headings_by_level\":[{}],",
            self.structure.headings_by_level.iter().map(|n| n.to_string()).collect::<Vec<_>>().join(",")
        ));
        out.push_str(&format!("\"paragraphs\":{},", self.structure.paragraphs));
        out.push_str(&format!("\"code_blocks\":{},", self.structure.code_blocks));
        out.push_str(&format!(
            "\"code_languages\":[{}],",
            self.structure.code_languages.iter().map(|l| format!("\"{}\"", json_escape(l))).collect::<Vec<_>>().join(",")
        ));
        out.push_str(&format!("\"tables\":{},", self.structure.tables));
        out.push_str(&format!("\"table_rows\":{},", self.structure.table_rows));
        out.push_str(&format!("\"table_cells\":{},", self.structure.table_cells));
        out.push_str(&format!("\"lists\":{},", self.structure.lists));
        out.push_str(&format!("\"list_items\":{},", self.structure.list_items));
        out.push_str(&format!("\"task_items_total\":{},", self.structure.task_items_total));
        out.push_str(&format!("\"task_items_completed\":{},", self.structure.task_items_completed));
        out.push_str(&format!("\"blockquotes\":{},", self.structure.blockquotes));
        out.push_str(&format!("\"callouts_total\":{},", self.structure.callouts_total));
        out.push_str("\"callouts_by_kind\":{");
        let callout_entries: Vec<String> = self
            .structure
            .callouts_by_kind
            .iter()
            .map(|(k, v)| format!("\"{}\":{}", json_escape(k), v))
            .collect();
        out.push_str(&callout_entries.join(","));
        out.push_str("},");
        out.push_str(&format!("\"math_blocks\":{},", self.structure.math_blocks));
        out.push_str(&format!("\"math_inlines\":{},", self.structure.math_inlines));
        out.push_str(&format!("\"links_total\":{},", self.structure.links_total));
        out.push_str(&format!("\"links_external\":{},", self.structure.links_external));
        out.push_str(&format!("\"links_internal_anchors\":{},", self.structure.links_internal_anchors));
        out.push_str(&format!("\"images\":{},", self.structure.images));
        out.push_str(&format!("\"footnote_definitions\":{},", self.structure.footnote_definitions));
        out.push_str(&format!("\"footnote_references\":{}", self.structure.footnote_references));
        out.push_str("},");

        // Outline
        out.push_str("\"outline\":[");
        let outline_items: Vec<String> = self
            .outline
            .iter()
            .map(|h| {
                format!(
                    "{{\"level\":{},\"text\":\"{}\",\"slug\":\"{}\"}}",
                    h.level,
                    json_escape(&h.text),
                    json_escape(&h.slug)
                )
            })
            .collect();
        out.push_str(&outline_items.join(","));
        out.push_str("],");

        // Findings
        out.push_str("\"findings\":[");
        let finding_items: Vec<String> = self
            .findings
            .iter()
            .map(|f| {
                format!(
                    "{{\"severity\":\"{}\",\"code\":\"{}\",\"message\":\"{}\"}}",
                    f.severity,
                    f.code,
                    json_escape(&f.message)
                )
            })
            .collect();
        out.push_str(&finding_items.join(","));
        out.push_str("]");

        out.push('}');
        out
    }

    /// Emit formatted human-readable report.
    #[must_use]
    pub fn to_human_report(&self) -> String {
        let mut s = String::with_capacity(2048);
        s.push_str("=== Document Intelligence Report ===\n\n");
        s.push_str(&format!(
            "Size:        {} words | {} characters | {} lines | {} bytes\n",
            self.words, self.characters, self.lines, self.bytes
        ));
        let read_mins = self.reading_time_secs / 60;
        let read_rem_secs = self.reading_time_secs % 60;
        let speak_mins = self.speaking_time_secs / 60;
        let speak_rem_secs = self.speaking_time_secs % 60;
        s.push_str(&format!(
            "Pacing:      Reading: {}m {:02}s | Speaking: {}m {:02}s\n",
            read_mins, read_rem_secs, speak_mins, speak_rem_secs
        ));
        s.push_str(&format!(
            "Readability: Flesch Reading Ease: {:.1} ({})\n             Flesch-Kincaid Grade: Grade {:.1}\n\n",
            self.flesch_reading_ease, self.reading_ease_label, self.flesch_kincaid_grade
        ));

        s.push_str("--- Structure ---\n");
        s.push_str(&format!(
            "Headings:    {} (H1: {}, H2: {}, H3: {}, H4: {}, H5: {}, H6: {})\n",
            self.structure.headings_total,
            self.structure.headings_by_level[0],
            self.structure.headings_by_level[1],
            self.structure.headings_by_level[2],
            self.structure.headings_by_level[3],
            self.structure.headings_by_level[4],
            self.structure.headings_by_level[5]
        ));
        s.push_str(&format!(
            "Paragraphs:  {} | Blockquotes: {} | Callouts: {}\n",
            self.structure.paragraphs, self.structure.blockquotes, self.structure.callouts_total
        ));
        if !self.structure.callouts_by_kind.is_empty() {
            let kinds: Vec<String> = self
                .structure
                .callouts_by_kind
                .iter()
                .map(|(k, v)| format!("{k}: {v}"))
                .collect();
            s.push_str(&format!("             ({})\n", kinds.join(", ")));
        }
        s.push_str(&format!(
            "Code Blocks: {} | Languages: {}\n",
            self.structure.code_blocks,
            if self.structure.code_languages.is_empty() {
                "none".to_string()
            } else {
                self.structure.code_languages.join(", ")
            }
        ));
        s.push_str(&format!(
            "Tables:      {} ({} rows, {} cells)\n",
            self.structure.tables, self.structure.table_rows, self.structure.table_cells
        ));
        s.push_str(&format!(
            "Lists:       {} ({} items, {} tasks [{} completed])\n",
            self.structure.lists,
            self.structure.list_items,
            self.structure.task_items_total,
            self.structure.task_items_completed
        ));
        s.push_str(&format!(
            "Links:       {} ({} external, {} internal anchors)\n",
            self.structure.links_total,
            self.structure.links_external,
            self.structure.links_internal_anchors
        ));
        s.push_str(&format!(
            "Images:      {} | Math Blocks: {} | Footnotes: {}\n\n",
            self.structure.images, self.structure.math_blocks, self.structure.footnote_definitions
        ));

        if !self.outline.is_empty() {
            s.push_str("--- Outline ---\n");
            for h in &self.outline {
                let indent = "  ".repeat((h.level.saturating_sub(1)) as usize);
                s.push_str(&format!("{}H{} {} (#{})\n", indent, h.level, h.text, h.slug));
            }
            s.push('\n');
        }

        if !self.findings.is_empty() {
            s.push_str(&format!("--- Findings ({} issues) ---\n", self.findings.len()));
            for f in &self.findings {
                s.push_str(&format!("[{}] ({}): {}\n", f.severity.to_uppercase(), f.code, f.message));
            }
        } else {
            s.push_str("--- Health: Clean (0 issues) ---\n");
        }

        s
    }
}

fn json_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for ch in s.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => {
                use std::fmt::Write;
                let _ = write!(out, "\\u{:04x}", c as u32);
            }
            c => out.push(c),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse_markdown;

    #[test]
    fn compute_doc_stats_calculates_metrics_and_readability() {
        let md = r#"# Document Title

This is a clear introductory paragraph. It explains the architecture of the system.

## Features

- [x] Fast rendering
- [ ] Adaptive layout
- Comprehensive test suite

> [!NOTE]
> Important operational notice.

| Feature | Status |
| --- | --- |
| HTML | Stable |
| PDF | Stable |

Visit our [Home Page](https://example.com) or read the [Features Section](#features).
"#;
        let doc = parse_markdown(md);
        let stats = compute_doc_stats(md, &doc);

        assert!(stats.words >= 40);
        assert!(stats.sentences >= 4);
        assert_eq!(stats.structure.headings_total, 2);
        assert_eq!(stats.structure.headings_by_level[0], 1); // H1
        assert_eq!(stats.structure.headings_by_level[1], 1); // H2
        assert_eq!(stats.structure.callouts_total, 1);
        assert_eq!(stats.structure.tables, 1);
        assert_eq!(stats.structure.task_items_total, 2);
        assert_eq!(stats.structure.task_items_completed, 1);
        assert_eq!(stats.structure.links_external, 1);
        assert_eq!(stats.structure.links_internal_anchors, 1);
        assert!(stats.findings.is_empty()); // No broken links or skips

        let json = stats.to_json();
        assert!(json.contains("\"schema\":\"fmd-document-stats-v1\""));
        assert!(json.contains("\"headings_total\":2"));
        assert!(json.contains("\"callouts_total\":1"));

        let report = stats.to_human_report();
        assert!(report.contains("Document Intelligence Report"));
        assert!(report.contains("H1 Document Title (#document-title)"));
        assert!(report.contains("Health: Clean"));
    }

    #[test]
    fn compute_doc_stats_detects_broken_anchors_and_hierarchy_skips() {
        let md = r#"# Level 1

### Level 3 Skipped H2

Paragraph with [Broken Link](#nonexistent-anchor) and [Footnote Reference][^1].
"#;
        let doc = parse_markdown(md);
        let stats = compute_doc_stats(md, &doc);

        assert!(stats.findings.iter().any(|f| f.code == "heading_hierarchy_skip"));
        assert!(stats.findings.iter().any(|f| f.code == "broken_internal_anchor"));
        assert!(stats.findings.iter().any(|f| f.code == "undefined_footnote"));
    }
}
