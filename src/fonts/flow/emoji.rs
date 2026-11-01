//! Bounded admission for the existing curated monochrome emoji face.
//!
//! Cmap coverage is always required as well. These ranges cannot admit the
//! font's zero-width formatting glyphs as independent visible characters,
//! regional indicators or skin-tone modifiers as detached components.

pub(super) fn scalar(ch: char) -> bool {
    matches!(ch as u32,
        0x00a9 | 0x00ae | 0x203c | 0x2049 |
        0x2100..=0x23ff | 0x25a0..=0x27bf | 0x2934..=0x2935 |
        0x2b00..=0x2bff | 0x3030 | 0x303d | 0x3297 | 0x3299 |
        0x1f000..=0x1faff
    ) && !matches!(ch as u32, 0x1f1e6..=0x1f1ff | 0x1f3fb..=0x1f3ff)
}

/// The standardized Unicode 17 emoji variation bases covered by the bundled
/// curated cmap. This is a fixed admission subset, not general variation-font
/// support. Both FE0E and FE0F are standardized for each listed base.
///
/// Source: https://www.unicode.org/Public/17.0.0/ucd/emoji/emoji-variation-sequences.txt
/// Source SHA-256: bb3d09ef03f206012c7532dd52dc0a21c9efddba0135ea4cf0d9201b8b9bba7e
pub(super) fn variation_base(ch: char) -> bool {
    const BASES: &str = concat!(
        "©®‼⁉™ℹ↔↕↖↗↘↙↩↪⌛⌨⏏⏩⏪⏫⏬⏭⏮⏯⏰⏱⏲⏳⏸⏹⏺▶◀☀☁☑",
        "☕☝♻♿⚖⚙⚠⚡⚪⚫⛅⛔✅✈✉✋✌✏✒✔✖✨✳✴❄❇❌❎❓❔❕❗❤➕➖➗",
        "➡⤴⤵⬅⬆⬇⬛⬜⭐〽🌍🌎🌏🎖🎧🎬🎮🏆🏗🏠🏳🏷👁👆👇👈👉👍👎💻💿📋📚📦📫📬",
        "📷📺📻🔍🔒🔓🕐🖊🖋🖥🖱🗂🗃🗄🗑🗓🗝🗞🗨🗯🗺😐🚑🛠🛡🛰",
    );
    BASES.contains(ch)
}
