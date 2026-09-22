//! Measured hanging gutters at the active body size, including custom faces.

use franken_markdown::ast::List;
use super::{Piece, Poster, RStyle, SLOT_BODY};

impl Poster {
    pub(super) fn list(&mut self, list: &List, l: f64, r: f64, quote: bool) {
        let size = f64::from(self.scale.body);
        let style = RStyle { ink: Self::default_ink(quote), ..RStyle::BODY };
        let bullet = self.faces[SLOT_BODY].as_ref().map_or("-", |font| {
            if font.glyph_index('•') != 0 { "•" } else { "-" }
        });
        let markers: Vec<String> = list.items.iter().enumerate().map(|(index, item)| {
            match item.task {
                Some(true) => "[x]".to_owned(),
                Some(false) => "[ ]".to_owned(),
                // A host-constructed AST may start at u64::MAX. Arithmetic in
                // u128 preserves subsequent ordinals instead of wrapping/panicking.
                None if list.ordered => format!("{}.", u128::from(list.start) + index as u128),
                None => bullet.to_owned(),
            }
        }).collect();
        let gap = self.space_width(style, size).max(size * 0.2);
        let gutter = markers.iter().map(|marker| self.measure(marker, style, size) + gap)
            .fold(18.0 * self.unit_scale(), f64::max);
        let hanging = gutter + size <= r - l;
        for (item, marker) in list.items.iter().zip(&markers) {
            let top = self.y;
            if !hanging {
                // Do not steal the remaining body measure or overlap a wide
                // ordinal with its content. Put the complete marker on its own
                // wrapped line when a usable hanging layout cannot fit.
                self.text_lines(&[Piece::Text(marker.clone(), style)], size, l, r, quote, 0.0);
            }
            for block in &item.blocks {
                self.block(block, if hanging { l + gutter } else { l }, r, quote);
            }
            if hanging {
                self.draw_text(l, top + size * 0.85, marker, style, size);
            }
            // Empty items and containers emptied by endnote preparation still
            // need a line: their markers must never occupy the same baseline.
            self.y = self.y.max(top + size * self.line_height);
            if !list.tight {
                self.y += 4.0 * self.unit_scale();
            }
        }
        self.y += size * 0.3;
    }
}
