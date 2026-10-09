//! Regenerate the bundled emoji fallback face (`fonts/noto-emoji/`).
//!
//! The text faces carry no emoji, so README-style documents (`🚀 Quick start`,
//! `✅ Done`, `⚠️ Warning`) printed `.notdef` boxes in PDF. This generator
//! instances the monochrome Noto Emoji variable font at Regular weight and
//! subsets it with the project's own clean-room subsetter to the curated
//! repertoire below — the emoji documentation actually uses — so the face
//! stays around 110 KiB. The PDF embedder subsets it again per document.
//!
//! The face is native-only (cargo feature `emoji-face`, on with `cli`): it is
//! too large for the WASM size budget, and browsers draw emoji themselves.
//!
//! Usage:
//!
//! ```text
//! cargo run --example gen_emoji_fallback_font -- \
//!     /path/to/NotoEmoji[wght].ttf fmd-font/fonts/noto-emoji/NotoEmojiCurated.ttf
//! ```
//!
//! Source font: Noto Emoji (SIL OFL 1.1), from
//! <https://github.com/google/fonts/tree/main/ofl/notoemoji>. Regenerating
//! from the same source release is deterministic.

use franken_markdown::text::Font;
use std::process::ExitCode;

/// The curated repertoire, grouped for review. Single codepoints only: the
/// layout engine draws one glyph per character, so ZWJ sequences render as
/// their component emoji and skin-tone modifiers are not carried.
const CURATED: &str = concat!(
    // Status, checks and alerts.
    "✅☑✔✖❌❎⚠⛔🚫🚨❗❓❕❔‼⁉ℹ💯🆗🆕🆙🆓🔴🟠🟡🟢🔵🟣🟤⚫⚪🟥🟧🟨🟩🟦🟪⬛⬜🔶🔷🔸🔹",
    // Software and project work.
    "🚀✨🎉🎊🔥💡📝🐛🔧🔨🛠⚙⚡🎯📦📚📖📘📗📕📙🔒🔓🔐🔑🗝🧪🔬🔍🔎🧰🧩🧱🏗🚧🚦",
    "💻🖥⌨🖱📱🔌🔋💾💿📀🗄🗃🗂📁📂📄📃📑📋📎📌📍🔗🏷🔖📊📈📉🧮",
    "🤖👾🐳🐧🦀🐍🐘🦊🐙🐱🐶🦄🐝🍎🪟☁🌐🌍🌎🌏🗺🧭📡🛰",
    "🔄🔁🔃♻🔀↩↪⤴⤵⬆⬇⬅➡↗↘↙↖↕↔🔼🔽⏫⏬⏩⏪▶◀⏸⏹⏺⏭⏮⏯",
    "➕➖➗✳✴❇〽💲💱©®™🔟🔢🔣🔤🔠🔡",
    // Communication and docs.
    "💬💭🗨🗯📣📢🔔🔕📬📫📮📧✉📨📩✏✒🖊🖋📜🧾📰🗞📅📆🗓⏰⏱⏲⌛⏳🕐",
    // People and gestures.
    "👍👎👏🙌🙏👋✋🤚👌✌🤞🤝💪👀👁👉👈👆👇☝🧠👤👥👷🧑👨👩🙈🙉🙊",
    "😀😃😄😁😆😅😂🤣😊😇🙂😉😍🤩😘😎🤓🧐🤔🤨😐😑😶🙄😏😬😴😷🤯😱😢😭😤😡🥳🥺😮",
    // Symbols and decoration.
    "❤🧡💛💚💙💜🖤🤍🤎💔💖💥💫⭐🌟✨🏆🥇🥈🥉🏅🎖🎁🎈🎀🎨🎵🎶🎤🎧🎬🎮🎲",
    "☀🌙⭐⛅🌈❄🔆🌱🌲🌳🍀🌸🌻☕🍕🍔🍺🍰🧁🚗🚚✈🚢🏠🏢🏁🚩🏳🎌",
    "🛡⚖🧹🗑🚑🩹💊💉🔊🔇📷📸🎥📺📻⏏🔅🔆♿🚻🛑",
);

/// Invisible formatting characters that must map to zero-width glyphs rather
/// than `.notdef`: variation selectors, the zero-width joiner and the keycap
/// combiner. Skipped (and reported) if the source font does not map them.
const FORMAT_CHARS: &[char] = &['\u{FE0E}', '\u{FE0F}', '\u{200D}', '\u{20E3}'];

/// The generator fails if any of these is missing from the source font.
const REQUIRED: &[char] = &['🚀', '✅', '❌', '⚠', '✨', '🎉', '🔥', '💡', '📦', '👍'];

fn run(source_path: &str, output_path: &str) -> Result<(), String> {
    let bytes = std::fs::read(source_path).map_err(|e| format!("reading {source_path}: {e}"))?;
    let font = Font::parse(bytes).map_err(|e| format!("parsing {source_path}: {e}"))?;
    let font = if font.instance_bounds(*b"wght").is_some() {
        font.instance(400.0)
            .ok_or_else(|| format!("instancing {source_path} at wght 400 failed"))?
    } else {
        font
    };
    for &c in REQUIRED {
        if font.glyph_index(c) == 0 {
            return Err(format!(
                "source font lacks required emoji {c:?} (U+{:04X})",
                u32::from(c)
            ));
        }
    }
    let mut keep: Vec<char> = CURATED
        .chars()
        .chain(FORMAT_CHARS.iter().copied())
        .collect();
    keep.sort_unstable();
    keep.dedup();
    let (kept, skipped): (Vec<char>, Vec<char>) =
        keep.into_iter().partition(|&c| font.glyph_index(c) != 0);
    let subset = font
        .subset(&kept)
        .ok_or_else(|| format!("subsetting {source_path} failed"))?;
    let check = Font::parse(subset.clone()).map_err(|e| format!("re-parsing subset: {e}"))?;
    for &c in &kept {
        if check.glyph_index(c) == 0 {
            return Err(format!("subset lost {c:?} (U+{:04X})", u32::from(c)));
        }
    }
    std::fs::write(output_path, &subset).map_err(|e| format!("writing {output_path}: {e}"))?;
    println!(
        "wrote {output_path}: {} glyph codepoints, {} bytes",
        kept.len(),
        subset.len()
    );
    if !skipped.is_empty() {
        let shown: Vec<String> = skipped
            .iter()
            .map(|c| format!("U+{:04X}", u32::from(*c)))
            .collect();
        println!("skipped (not in source font): {}", shown.join(" "));
    }
    Ok(())
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().collect();
    let [_, source, output] = args.as_slice() else {
        eprintln!("usage: gen_emoji_fallback_font <NotoEmoji[wght].ttf> <output.ttf>");
        return ExitCode::from(64);
    };
    match run(source, output) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("gen_emoji_fallback_font: {error}");
            ExitCode::FAILURE
        }
    }
}
