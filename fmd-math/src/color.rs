//! xcolor colour specifications for `\color` and `\textcolor`
//! (franken_manim fm-tex-color-silent-i0p2).
//!
//! The surface is what LaTeX's plain `\usepackage{xcolor}` provides, which
//! is what the Reference's Tex templates load:
//!
//! - the 19 base colour names (xcolor v3.02, `xcolor.sty` lines 1434-1455),
//!   case-sensitive;
//! - colour expressions over them: `name!pct!name…` mixes left to right,
//!   a trailing `!pct` (or an empty name) mixes with white, and a leading
//!   `-` takes the complement of the result. Every mix is linear in RGB,
//!   which is what xcolor computes for these names (checked with
//!   `\convertcolorspec` under xcolor v3.02);
//! - the explicit models `[rgb]{r,g,b}` (0..1), `[RGB]{R,G,B}` (0..255),
//!   `[HTML]{RRGGBB}` and `[gray]{g}` (0..1).
//!
//! Anything else (dvipsnames/svgnames, `\definecolor`, other models,
//! postfix operators) is a precise error naming what is supported. Never a
//! silent fallback colour.

use crate::error::MathError;
use crate::mbox::Color;
use crate::node::ColorSpec;

/// xcolor's base colours, as RGB in 0..1 (the gray-model names carry
/// r = g = b).
const BASE: [(&str, [f64; 3]); 19] = [
    ("red", [1.0, 0.0, 0.0]),
    ("green", [0.0, 1.0, 0.0]),
    ("blue", [0.0, 0.0, 1.0]),
    ("brown", [0.75, 0.5, 0.25]),
    ("lime", [0.75, 1.0, 0.0]),
    ("orange", [1.0, 0.5, 0.0]),
    ("pink", [1.0, 0.75, 0.75]),
    ("purple", [0.75, 0.0, 0.25]),
    ("teal", [0.0, 0.5, 0.5]),
    ("violet", [0.5, 0.0, 0.5]),
    ("cyan", [0.0, 1.0, 1.0]),
    ("magenta", [1.0, 0.0, 1.0]),
    ("yellow", [1.0, 1.0, 0.0]),
    ("olive", [0.5, 0.5, 0.0]),
    ("black", [0.0, 0.0, 0.0]),
    ("darkgray", [0.25, 0.25, 0.25]),
    ("gray", [0.5, 0.5, 0.5]),
    ("lightgray", [0.75, 0.75, 0.75]),
    ("white", [1.0, 1.0, 1.0]),
];

const SUPPORTED: &str = "xcolor's base names (red, green, blue, brown, lime, orange, pink, \
     purple, teal, violet, cyan, magenta, yellow, olive, black, darkgray, gray, lightgray, \
     white) with `!` mixes and a leading `-`, or the [rgb], [RGB], [HTML] and [gray] models";

/// One `\color` / `\textcolor` declaration. An explicit `[model]` is new
/// syntax and must resolve now (a parse error otherwise); MathML then carries
/// its `#rrggbb`. A bare colour keeps its verbatim spelling for MathML, where
/// browsers read CSS names and hex, and stores its xcolor resolution, so only
/// layout refuses a colour xcolor does not define.
pub(crate) fn declaration(
    command: &str,
    model: Option<&str>,
    spec: &str,
    at: usize,
) -> Result<ColorSpec, MathError> {
    if model.is_some() {
        let color = resolve(command, model, spec, at)?;
        return Ok(ColorSpec {
            spec: hex(color),
            color: Ok(color),
        });
    }
    Ok(ColorSpec {
        spec: spec.trim().to_owned(),
        color: resolve(command, None, spec, at),
    })
}

/// `#rrggbb`, each channel rounded to 8 bits.
pub(crate) fn hex(color: Color) -> String {
    let byte = |v: f64| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
    format!(
        "#{:02x}{:02x}{:02x}",
        byte(color.r),
        byte(color.g),
        byte(color.b)
    )
}

/// Resolve one colour argument: `model` is the optional `[…]` argument,
/// `spec` the braced one, `at` the byte offset errors point at; `command`
/// names the command for the messages (`\color`, `\textcolor`).
pub(crate) fn resolve(
    command: &str,
    model: Option<&str>,
    spec: &str,
    at: usize,
) -> Result<Color, MathError> {
    let fail = |what: String| MathError::Malformed { what, at };
    let spec = spec.trim();
    let rgb = match model.map(str::trim) {
        None => expression(spec).ok_or_else(|| {
            fail(format!(
                "unsupported colour `{spec}` in {command}: use {SUPPORTED}"
            ))
        })?,
        Some("rgb") => {
            let [r, g, b] = components::<3>(spec)
                .filter(|values| values.iter().all(|v| (0.0..=1.0).contains(v)))
                .ok_or_else(|| {
                    fail(format!(
                        "{command}[rgb] takes three numbers from 0 to 1, got `{spec}`"
                    ))
                })?;
            [r, g, b]
        }
        Some("RGB") => {
            let values = components::<3>(spec)
                .filter(|values| {
                    values
                        .iter()
                        .all(|v| v.fract() == 0.0 && (0.0..=255.0).contains(v))
                })
                .ok_or_else(|| {
                    fail(format!(
                        "{command}[RGB] takes three integers from 0 to 255, got `{spec}`"
                    ))
                })?;
            values.map(|v| v / 255.0)
        }
        Some("HTML") => html(spec).ok_or_else(|| {
            fail(format!(
                "{command}[HTML] takes six hexadecimal digits, got `{spec}`"
            ))
        })?,
        Some("gray") => {
            let [g] = components::<1>(spec)
                .filter(|values| (0.0..=1.0).contains(&values[0]))
                .ok_or_else(|| {
                    fail(format!(
                        "{command}[gray] takes one number from 0 to 1, got `{spec}`"
                    ))
                })?;
            [g, g, g]
        }
        Some(other) => {
            return Err(fail(format!(
                "unsupported colour model `{other}` in {command}: use {SUPPORTED}"
            )));
        }
    };
    Ok(Color {
        r: rgb[0],
        g: rgb[1],
        b: rgb[2],
    })
}

fn base(name: &str) -> Option<[f64; 3]> {
    if name.is_empty() {
        // xcolor mixes an empty name as white (`red!30!` = `red!30`).
        return Some([1.0, 1.0, 1.0]);
    }
    BASE.iter().find(|(n, _)| *n == name).map(|(_, rgb)| *rgb)
}

/// A colour expression: `-`* name (`!` pct (`!` name)?)*.
fn expression(spec: &str) -> Option<[f64; 3]> {
    let body = spec.trim_start_matches('-');
    let complement = (spec.len() - body.len()) % 2 == 1;
    let mut parts = body.split('!');
    let first = parts.next()?.trim();
    if first.is_empty() {
        return None;
    }
    let mut rgb = base(first)?;
    while let Some(pct) = parts.next() {
        let pct = percent(pct)?;
        let other = base(parts.next().unwrap_or("").trim())?;
        let t = pct / 100.0;
        rgb = [0, 1, 2].map(|i| t * rgb[i] + (1.0 - t) * other[i]);
    }
    if complement {
        rgb = rgb.map(|v| 1.0 - v);
    }
    Some(rgb)
}

fn percent(text: &str) -> Option<f64> {
    let value = number(text)?;
    (0.0..=100.0).contains(&value).then_some(value)
}

/// A TeX decimal: digits with at most one point, either side may be empty
/// (`.5`, `5.`), no sign, no exponent.
fn number(text: &str) -> Option<f64> {
    let text = text.trim();
    let digits = text.chars().filter(char::is_ascii_digit).count();
    let points = text.chars().filter(|c| *c == '.').count();
    if digits == 0 || points > 1 || digits + points != text.len() {
        return None;
    }
    text.parse().ok()
}

fn components<const N: usize>(spec: &str) -> Option<[f64; N]> {
    let values: Vec<f64> = spec.split(',').map(number).collect::<Option<_>>()?;
    values.try_into().ok()
}

fn html(spec: &str) -> Option<[f64; 3]> {
    let spec = spec.trim();
    if spec.len() != 6 || !spec.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    let byte = |i: usize| u8::from_str_radix(&spec[i..i + 2], 16).ok();
    Some([byte(0)?, byte(2)?, byte(4)?].map(|v| f64::from(v) / 255.0))
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::panic)]
mod tests {
    use super::*;

    fn rgb(model: Option<&str>, spec: &str) -> [f64; 3] {
        let c = resolve("\\color", model, spec, 0).unwrap();
        [c.r, c.g, c.b]
    }

    fn close(a: [f64; 3], b: [f64; 3]) -> bool {
        a.iter().zip(b).all(|(x, y)| (x - y).abs() < 1e-12)
    }

    #[test]
    fn expressions_match_xcolor_3_02() {
        // Each expected value is what `\convertcolorspec{…}{rgb}` printed
        // under xcolor v3.02 (TeX Live 2024) for the same expression.
        for (spec, want) in [
            ("red", [1.0, 0.0, 0.0]),
            ("orange", [1.0, 0.5, 0.0]),
            ("cyan", [0.0, 1.0, 1.0]),
            ("olive", [0.5, 0.5, 0.0]),
            ("gray", [0.5, 0.5, 0.5]),
            ("red!30!blue", [0.3, 0.0, 0.7]),
            ("red!30", [1.0, 0.7, 0.7]),
            ("blue!70", [0.3, 0.3, 1.0]),
            ("black!50!red", [0.5, 0.0, 0.0]),
            ("red!50!black", [0.5, 0.0, 0.0]),
            ("red!25!blue!50!green", [0.125, 0.5, 0.375]),
            ("-red", [0.0, 1.0, 1.0]),
            ("-gray", [0.5, 0.5, 0.5]),
            ("white!50!blue", [0.5, 0.5, 1.0]),
            ("red!0", [1.0, 1.0, 1.0]),
            ("red!100!blue", [1.0, 0.0, 0.0]),
            ("brown!60!lightgray", [0.75, 0.6, 0.45]),
        ] {
            assert!(
                close(rgb(None, spec), want),
                "{spec}: {:?}",
                rgb(None, spec)
            );
        }
    }

    #[test]
    fn explicit_models() {
        assert!(close(rgb(Some("rgb"), "1, .5,0"), [1.0, 0.5, 0.0]));
        assert!(close(
            rgb(Some("RGB"), "0, 128, 255"),
            [0.0, 128.0 / 255.0, 1.0]
        ));
        assert!(close(
            rgb(Some("HTML"), "FF8800"),
            [1.0, 136.0 / 255.0, 0.0]
        ));
        assert!(close(
            rgb(Some("HTML"), "ff8800"),
            [1.0, 136.0 / 255.0, 0.0]
        ));
        assert!(close(rgb(Some("gray"), ".25"), [0.25, 0.25, 0.25]));
    }

    #[test]
    fn declarations_keep_html_spellings_and_defer_the_xcolor_refusal() {
        // A CSS name and a hex spelling still reach MathML verbatim; only
        // their xcolor resolution fails, for layout to report.
        for raw in ["steelblue", "#00ff00"] {
            let css = declaration("\\color", None, raw, 3).unwrap();
            assert_eq!(css.spec, raw);
            assert!(css.color.is_err(), "{raw}");
        }
        let named = declaration("\\color", None, " red!30!blue ", 3).unwrap();
        assert_eq!(named.spec, "red!30!blue");
        assert!(named.color.is_ok());
        // An explicit model resolves now and writes its hex.
        let model = declaration("\\color", Some("RGB"), "0,128,255", 3).unwrap();
        assert_eq!(model.spec, "#0080ff");
        assert!(declaration("\\color", Some("RGB"), "0,128", 3).is_err());
    }

    #[test]
    fn everything_else_is_a_precise_error() {
        for (model, spec, needle) in [
            (None, "RED", "unsupported colour `RED` in \\color"),
            (None, "Red", "unsupported colour `Red`"),
            (None, "red!130!blue", "unsupported colour `red!130!blue`"),
            (None, "red!x", "unsupported colour"),
            (None, "", "unsupported colour ``"),
            (None, "!50", "unsupported colour"),
            (Some("rgb"), "1,2,0", "three numbers from 0 to 1"),
            (Some("rgb"), "1,0", "three numbers from 0 to 1"),
            (Some("RGB"), "0,1.5,3", "three integers from 0 to 255"),
            (Some("RGB"), "0,0,256", "three integers from 0 to 255"),
            (Some("HTML"), "FF88", "six hexadecimal digits"),
            (Some("gray"), "-1", "one number from 0 to 1"),
            (Some("cmyk"), "0,1,1,0", "unsupported colour model `cmyk`"),
        ] {
            let err = resolve("\\color", model, spec, 7).unwrap_err();
            let MathError::Malformed { what, at } = &err else {
                panic!("{spec}: {err:?}");
            };
            assert!(what.contains(needle), "{spec}: {what}");
            assert_eq!(*at, 7);
        }
    }
}
