//! Shared presentation for explicitly opted-in financial pipe tables.
//!
//! Source rows remain in the AST for source maps and editing. Only a table with
//! a decimal column interprets plain marker rows as footing decorations.

use crate::ast::{Align, Inline, Table};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum FootingRule {
    #[default]
    None,
    Single,
    Double,
}

pub(crate) struct FinancialRow<'a> {
    pub source_index: usize,
    pub cells: &'a [Vec<Inline>],
    pub rules: Vec<FootingRule>,
}

pub(crate) fn financial_rows(table: &Table) -> Option<Vec<FinancialRow<'_>>> {
    if !table.align.contains(&Align::Decimal) {
        return None;
    }
    let columns = table
        .rows
        .iter()
        .map(Vec::len)
        .chain([table.align.len(), table.head.len()])
        .max()
        .unwrap_or(0);
    let mut rows: Vec<FinancialRow<'_>> = Vec::with_capacity(table.rows.len());
    for (source_index, cells) in table.rows.iter().enumerate() {
        if let Some(rules) = marker_rules(cells, columns)
            && let Some(previous) = rows.last_mut()
        {
            for (old, new) in previous.rules.iter_mut().zip(rules) {
                *old = (*old).max(new);
            }
            continue;
        }
        rows.push(FinancialRow {
            source_index,
            cells,
            rules: vec![FootingRule::None; columns],
        });
    }
    Some(rows)
}

fn marker_rules(cells: &[Vec<Inline>], columns: usize) -> Option<Vec<FootingRule>> {
    let mut rules = vec![FootingRule::None; columns];
    let mut marked = false;
    for (column, cell) in cells.iter().enumerate() {
        let marker = match cell.as_slice() {
            [] => "",
            [Inline::Text(text)] => text.trim(),
            _ => return None,
        };
        rules[column] = match marker {
            "" => FootingRule::None,
            "---" => FootingRule::Single,
            "===" => FootingRule::Double,
            _ => return None,
        };
        marked |= rules[column] != FootingRule::None;
    }
    marked.then_some(rules)
}

/// Plain numeric spelling from text and inline styles that do not insert boxes.
/// Code, images, formulas and note marks keep their ordinary right alignment.
pub(crate) fn decimal_cell_text(cell: &[Inline]) -> Option<String> {
    fn append(cell: &[Inline], out: &mut String, depth: usize) -> Option<()> {
        if depth > 128 {
            return None;
        }
        for inline in cell {
            match inline {
                Inline::Text(text) => out.push_str(text),
                Inline::Emphasis(content)
                | Inline::Strong(content)
                | Inline::Strikethrough(content)
                | Inline::Link { content, .. } => append(content, out, depth + 1)?,
                _ => return None,
            }
        }
        Some(())
    }
    let mut text = String::new();
    append(cell, &mut text, 0)?;
    Some(text)
}

/// Return the byte boundary before the decimal point, or after an integer's
/// final digit. Values are never converted to numbers, rounded or reformatted.
///
/// Accepted spelling: ASCII digits, one optional dot, optional consistent
/// three-digit grouping (comma, apostrophe or space), signs, one currency
/// symbol, accounting parentheses, and an optional percent/per-mille suffix.
pub(crate) fn decimal_anchor(text: &str) -> Option<usize> {
    let mut body = text.trim();
    let mut start = text.len() - text.trim_start().len();
    if body.starts_with('(') {
        body = body.strip_prefix('(')?.strip_suffix(')')?;
        start += 1;
        let trimmed = body.trim_start();
        start += body.len() - trimmed.len();
        body = trimmed.trim_end();
    }
    if let Some(without) = body.strip_suffix('%').or_else(|| body.strip_suffix('‰')) {
        body = without.trim_end();
    }
    let mut sign = false;
    let mut currency = false;
    for _ in 0..2 {
        let first = body.chars().next()?;
        if matches!(first, '+' | '-' | '−') && !sign {
            sign = true;
        } else if matches!(first, '$' | '€' | '£' | '¥' | '₹' | '₩' | '₽') && !currency {
            currency = true;
        } else {
            break;
        }
        let rest = &body[first.len_utf8()..];
        body = rest.trim_start();
        start += first.len_utf8() + rest.len() - body.len();
    }
    if body.is_empty() {
        return None;
    }
    let dot = body.find('.');
    let integer = &body[..dot.unwrap_or(body.len())];
    let fraction = dot.map(|at| &body[at + 1..]);
    if fraction.is_some_and(|part| !part.bytes().all(|byte| byte.is_ascii_digit())) {
        return None;
    }
    if integer.is_empty() && fraction.is_none_or(str::is_empty) {
        return None;
    }
    let mut separator = None;
    let mut group_len = 0usize;
    let mut grouped = false;
    for character in integer.chars() {
        if character.is_ascii_digit() {
            group_len += 1;
        } else if matches!(character, ',' | '\'' | ' ' | '\u{00a0}' | '\u{202f}') {
            if group_len == 0 || (grouped && group_len != 3) || (!grouped && group_len > 3) {
                return None;
            }
            if separator.is_some_and(|old| old != character) {
                return None;
            }
            separator = Some(character);
            grouped = true;
            group_len = 0;
        } else {
            return None;
        }
    }
    if grouped && group_len != 3 {
        return None;
    }
    Some(start + dot.unwrap_or(body.len()))
}

/// Split accepted styled text at a visible-text byte boundary without flattening
/// its emphasis or links. Callers first use `decimal_cell_text` to admit it.
pub(crate) fn split_decimal_cell(cell: &[Inline], at: usize) -> (Vec<Inline>, Vec<Inline>) {
    fn split(cell: &[Inline], remaining: &mut usize) -> (Vec<Inline>, Vec<Inline>) {
        let mut before = Vec::new();
        let mut after = Vec::new();
        for inline in cell {
            if *remaining == 0 {
                after.push(inline.clone());
                continue;
            }
            match inline {
                Inline::Text(text) => {
                    let cut = (*remaining).min(text.len());
                    if text.is_char_boundary(cut) {
                        if cut > 0 {
                            before.push(Inline::Text(text[..cut].to_owned()));
                        }
                        if cut < text.len() {
                            after.push(Inline::Text(text[cut..].to_owned()));
                        }
                        *remaining -= cut;
                    }
                }
                Inline::Emphasis(content)
                | Inline::Strong(content)
                | Inline::Strikethrough(content)
                | Inline::Link { content, .. } => {
                    let (left, right) = split(content, remaining);
                    let wrap = |content| match inline {
                        Inline::Emphasis(_) => Inline::Emphasis(content),
                        Inline::Strong(_) => Inline::Strong(content),
                        Inline::Strikethrough(_) => Inline::Strikethrough(content),
                        Inline::Link { dest, title, .. } => Inline::Link {
                            dest: dest.clone(),
                            title: title.clone(),
                            content,
                        },
                        _ => unreachable!(),
                    };
                    if !left.is_empty() {
                        before.push(wrap(left));
                    }
                    if !right.is_empty() {
                        after.push(wrap(right));
                    }
                }
                _ => before.push(inline.clone()),
            }
        }
        (before, after)
    }
    let mut remaining = at;
    split(cell, &mut remaining)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn anchors_preserve_financial_spelling_and_utf8_boundaries() {
        for (text, prefix, suffix) in [
            ("1,234.5", "1,234", ".5"),
            ("(234.50)", "(234", ".50)"),
            (" −€ 12.50% ", " −€ 12", ".50% "),
            ("1\u{202f}234.567", "1\u{202f}234", ".567"),
            ("$42", "$42", ""),
            ("(42%)", "(42", "%)"),
            (".125", "", ".125"),
            ("-7.", "-7", "."),
        ] {
            let anchor = decimal_anchor(text).unwrap();
            assert_eq!((&text[..anchor], &text[anchor..]), (prefix, suffix));
        }
        for text in [
            "",
            "--",
            "12,34.5",
            "1,234 567",
            "1.2.3",
            "v1.2",
            "12kg",
            "(12",
            "12)",
            ".",
            "$",
            "12e3",
        ] {
            assert_eq!(decimal_anchor(text), None, "{text}");
        }
    }

    #[test]
    fn footing_rows_require_opt_in_and_preserve_source_identity() {
        let source = "| Item | Amount |\n| --- | ---. |\n| | === |\n| A | 12.3 |\n| | --- |\n| --- | === |\n| B | 7 |\n| code | `---` |\n";
        let doc = crate::parse_markdown(source);
        let crate::Block::Table(table) = &doc.blocks[0] else {
            panic!("financial table");
        };
        let rows = financial_rows(table).unwrap();
        assert_eq!(
            rows.iter().map(|row| row.source_index).collect::<Vec<_>>(),
            [0, 1, 4, 5]
        );
        assert_eq!(rows[0].rules, [FootingRule::None, FootingRule::None]);
        assert_eq!(rows[1].rules, [FootingRule::Single, FootingRule::Double]);
        let mut ordinary = table.clone();
        ordinary.align[1] = Align::Right;
        assert!(financial_rows(&ordinary).is_none());
    }

    #[test]
    fn splitting_a_number_keeps_styles_and_link_targets() {
        let cell = vec![Inline::Strong(vec![Inline::Link {
            dest: "#total".to_owned(),
            title: Some("Total".to_owned()),
            content: vec![Inline::Text("€12.30".to_owned())],
        }])];
        let text = decimal_cell_text(&cell).unwrap();
        let (before, after) = split_decimal_cell(&cell, decimal_anchor(&text).unwrap());
        assert_eq!(decimal_cell_text(&before).unwrap(), "€12");
        assert_eq!(decimal_cell_text(&after).unwrap(), ".30");
        assert!(
            matches!(&after[0], Inline::Strong(inner) if matches!(&inner[0], Inline::Link { dest, .. } if dest == "#total"))
        );
        assert!(decimal_cell_text(&[Inline::Code("12.3".to_owned())]).is_none());
    }
}
