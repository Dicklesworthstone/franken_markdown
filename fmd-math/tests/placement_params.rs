//! Placement-parameter fixtures: positions asserted against the PUBLISHED
//! Appendix-G values (the correctness spec is Knuth's, not pixels). Cases
//! are chosen so the rule's clearance terms do not bind, making the
//! published constant the exact expected coordinate.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
#![cfg(feature = "bundled-faces")]

use fmd_math::metrics::CM;
use fmd_math::{Engine, Layout, Style};

fn engine() -> Engine {
    match Engine::bundled() {
        Ok(e) => e,
        Err(e) => panic!("bundled faces: {e}"),
    }
}

fn glyph_y(layout: &Layout, ch: char) -> f64 {
    layout
        .glyphs
        .iter()
        .find(|g| g.ch == ch)
        .unwrap_or_else(|| panic!("glyph {ch} in {layout:?}"))
        .y
}

fn glyph_size(layout: &Layout, ch: char) -> f64 {
    layout
        .glyphs
        .iter()
        .find(|g| g.ch == ch)
        .unwrap_or_else(|| panic!("glyph {ch}"))
        .size
}

const EPS: f64 = 1e-9;

#[test]
fn text_ellipsis_uses_text_font_stretch_and_sentence_spacing() {
    let e = engine();
    let pack = fmd_math::MacroSet::pack("default").unwrap();
    // Independent default-template T1 showboxes: every text period is
    // followed by fontdimen3, including the last one. The explicit space
    // after an empty group retains the periods' sentence spacefactor.
    for (source, width) in [
        (r"\dots", 1.333_008),
        (r"\ldots", 1.333_008),
        (r"\textellipsis", 1.333_008),
        (r"a\dots b", 2.388_306),
        (r"a\dots{} b", 2.832_642),
    ] {
        let layout = e.typeset_text_with_macros(source, &pack).unwrap();
        assert!(
            (layout.width - width).abs() < 1e-4,
            "{source}: native {}, independent TeX {width}",
            layout.width
        );
        let dots: Vec<_> = layout.glyphs.iter().filter(|g| g.ch == '.').collect();
        assert_eq!(dots.len(), 3, "{source}");
        for pair in dots.windows(2) {
            assert!(
                (pair[1].x - pair[0].x - 0.444_336).abs() < 1e-5,
                "{source}: period positions {dots:?}"
            );
        }
        assert!(dots.iter().all(|g| {
            matches!(
                &source[g.span.start..g.span.end],
                r"\dots" | r"\ldots" | r"\textellipsis"
            )
        }));
    }
    // Math dots keep their math spacing, rather than becoming text dots.
    for (source, width) in [(r"$\dots$", 1.333_324), (r"$a\ldots b$", 2.457_741)] {
        let layout = e.typeset_text_with_macros(source, &pack).unwrap();
        assert!(
            (layout.width - width).abs() < 1e-4,
            "{source}: {}",
            layout.width
        );
    }
    let mono = e
        .typeset_text_with_macros(r"\texttt{\dots}", &pack)
        .unwrap();
    let period = e.typeset_text_with_macros(r"\texttt{.}", &pack).unwrap();
    assert!((mono.width - 1.574_615).abs() < 1e-4, "{mono:?}");
    assert!((mono.width - 3.0 * period.width).abs() < EPS);
    assert!(mono.glyphs.iter().all(|g| g.face == period.glyphs[0].face));
    let explicit_math = e
        .typeset_text_with_macros(r"$\textellipsis$", &pack)
        .unwrap();
    assert!((explicit_math.width - 1.333_214).abs() < 1e-4);
}

#[test]
fn text_dotso_keeps_amsmath_spacing_and_group_boundaries() {
    let e = engine();
    let pack = fmd_math::MacroSet::pack("default").unwrap();
    // Independent T1 showboxes with amsmath: unlike textellipsis,
    // dotso uses low math dots, leading text glue and thin math glue.
    // A brace/color boundary preserves its generated trailing space.
    for (source, expected) in [
        (r"\dotso", [1.666_576, 0.105_554, 0.0]),
        (r"a\dotso b", [3.055_125, 0.688_720, 0.0]),
        (r"{\dotso}", [1.999_828, 0.105_554, 0.0]),
        (r"{{\dotso}}", [1.999_828, 0.105_554, 0.0]),
        (r"\textcolor{red}{\dotso}", [1.999_828, 0.105_554, 0.0]),
        (r"\dotso,", [2.110_948, 0.105_554, 0.194_397]),
        (r"a \dotso, b", [3.499_498, 0.688_720, 0.194_397]),
        (r"{\dotso}b", [2.555_247, 0.688_720, 0.0]),
    ] {
        let layout = e.typeset_text_with_macros(source, &pack).unwrap();
        let actual = [layout.width, layout.height, layout.depth];
        for (got, want) in actual.into_iter().zip(expected) {
            assert!(
                (got - want).abs() < 1e-4,
                "{source}: native {actual:?}, independent TeX {expected:?}"
            );
        }
    }
}

#[test]
fn text_pair_kerns_follow_the_active_font_control() {
    // Independent T1 showbox captures without the Reference's font
    // control, and with it. Basic/bare callers keep natural text kerns.
    let e = engine();
    let basic = fmd_math::MacroSet::pack("basic").unwrap();
    let reference = fmd_math::MacroSet::pack("default").unwrap();
    for (source, natural, disabled) in [
        ("AV", 1.388_550, 1.499_634),
        ("Wave", 2.360_535, 2.499_390),
        ("Yes", 1.505_188, 1.588_501),
    ] {
        let bare = e.typeset_text(source).unwrap();
        let ordinary = e.typeset_text_with_macros(source, &basic).unwrap();
        let controlled = e.typeset_text_with_macros(source, &reference).unwrap();
        assert!(
            (bare.width - natural).abs() < 1e-5
                && (ordinary.width - natural).abs() < 1e-5
                && (controlled.width - disabled).abs() < 1e-5,
            "{source}: bare {}, basic {}, default {}, real TeX {natural}/{disabled}",
            bare.width,
            ordinary.width,
            controlled.width
        );
        let math = format!(r"\text{{{source}}}");
        assert!(
            (e.typeset(&math, Style::Text).unwrap().width - natural).abs() < 1e-5
                && (e
                    .typeset_with_macros(&math, Style::Text, &reference)
                    .unwrap()
                    .width
                    - disabled)
                    .abs()
                    < 1e-5,
            "{source}: math text island lost its caller's font control"
        );
    }
}

#[test]
fn bundled_upright_text_uses_t1_ec_boxes_and_kerns() {
    // Independent pdfTeX 1.40.28 showbox: article[10pt], fontenc[T1],
    // amsmath, with the default template's ligature-disable font control
    // applied to text. That control disables text kerns too. The operator
    // control stays in OT1 math roman.
    let e = engine();
    let reference = fmd_math::MacroSet::pack("default").unwrap();
    let mut mismatches = Vec::new();
    for (source, width, height, depth) in [
        (r"\text{Wave}", 2.499_390, 0.688_720, 0.0),
        (r"\text{AV}", 1.499_634, 0.688_720, 0.0),
        (r"\operatorname{AV}", 1.402_777, 0.683_331, 0.0),
        (r"x^{\text{AV}}", 1.801_794, 0.844_997, 0.0),
        (r"x^{y^{\text{AV}}}", 2.082_518, 1.009_032, 0.0),
        (r"\text{`}", 0.277_710, 0.688_720, 0.0),
        (r"y\quad0", 2.025_917, 0.644_444, 0.194_444),
        (r"x^{y\quad0}", 2.450_570, 0.814_003, 0.0),
    ] {
        let l = e
            .typeset_with_macros(source, Style::Text, &reference)
            .expect(source);
        if (l.width - width).abs() >= 1e-5
            || (l.height - height).abs() >= 1e-5
            || (l.depth - depth).abs() >= 1e-5
        {
            mismatches.push(format!(
                "{source}: ({}, {}, {}) vs ({width}, {height}, {depth})",
                l.width, l.height, l.depth
            ));
        }
    }
    assert!(mismatches.is_empty(), "{}", mismatches.join("\n"));
}

#[test]
fn text_mainland_uses_ec_at_declared_sizes_and_preserves_math_islands() {
    // The same independent showbox capture with the default template's
    // font control. Declared sizes select their own EC design, not scaled
    // ecrm1000. These measurements also exercise interword fontdimens.
    let e = engine();
    let reference = fmd_math::MacroSet::pack("default").unwrap();
    for (source, width, height) in [
        (r"{\tiny AV}", 0.980_316, 0.344_360),
        (r"{\scriptsize AV}", 1.180_267, 0.482_104),
        (r"{\footnotesize AV}", 1.273_022, 0.550_977),
        (r"{\small AV}", 1.386_880, 0.619_849),
        ("AV", 1.499_634, 0.688_720),
        (r"{\huge AV\normalsize AV}", 4.376_737, 1.428_217),
        (r"{\large AV}", 1.761_237, 0.826_465),
        (r"{\Large AV}", 2.056_192, 0.991_672),
        (r"{\LARGE AV}", 2.434_406, 1.189_987),
        (r"{\huge AV}", 2.877_103, 1.428_217),
        (r"{\Huge AV}", 3.406_796, 1.713_264),
        ("éßÆ", 1.846_771, 0.688_720),
        ("Yes", 1.588_501, 0.688_720),
        ("Wave", 2.499_390, 0.688_720),
        ("ffi fl", 1.805_115, 0.688_720),
        ("A V", 1.832_886, 0.688_720),
        (r"$\operatorname{AV}$", 1.402_777, 0.683_331),
    ] {
        let l = e
            .typeset_text_with_macros(source, &reference)
            .expect(source);
        assert!(
            (l.width - width).abs() < 1e-5
                && (l.height - height).abs() < 1e-5
                && l.depth.abs() < 1e-5,
            "{source}: ({}, {}, {}) vs ({width}, {height}, 0)",
            l.width,
            l.height,
            l.depth
        );
    }
}

#[test]
fn ec_metrics_preserve_explicit_glyphs_and_custom_face_metrics() {
    let e = engine();
    let reference = fmd_math::MacroSet::pack("default").unwrap();
    let l = e
        .typeset_text_with_macros("ffi fl éßÆ", &reference)
        .unwrap();
    assert_eq!(
        l.glyphs.iter().map(|g| g.ch).collect::<String>(),
        "ffifléßÆ"
    );
    // Reusing the same font bytes through the custom-roster API must not
    // silently opt the caller into bundled EC calibration.
    let custom = Engine::new(fmd_math::faces::FaceSet::from_fonts(vec![
        fmd_font::Font::parse(fmd_font::bundled::CM_REGULAR.to_vec()).unwrap(),
    ]));
    let l = custom.typeset_text("AV").unwrap();
    assert!((l.width - 1.5).abs() < EPS, "{}", l.width);
}

#[test]
fn custom_text_kerns_honor_the_active_font_control() {
    // A real bundled face with a known legacy kern table replaces its
    // GPOS table in memory. This exercises the custom-font parser and
    // layout route; no modified font is added to the shipped roster.
    let mut data = fmd_font::bundled::CM_REGULAR.to_vec();
    let original = fmd_font::Font::parse(data.clone()).unwrap();
    let left = original.glyph_index('A');
    let right = original.glyph_index('V');
    let count = usize::from(u16::from_be_bytes(data[4..6].try_into().unwrap()));
    let record = (0..count)
        .map(|i| 12 + 16 * i)
        .find(|&at| &data[at..at + 4] == b"GPOS")
        .expect("the bundled face has a GPOS table to replace");
    while data.len() & 3 != 0 {
        data.push(0);
    }
    let offset = u32::try_from(data.len()).unwrap();
    let mut table = Vec::new();
    for word in [0_u16, 1, 0, 20, 1, 1, 6, 0, 0, left, right] {
        table.extend_from_slice(&word.to_be_bytes());
    }
    table.extend_from_slice(&(-100_i16).to_be_bytes());
    let checksum = table.chunks_exact(4).fold(0_u32, |sum, word| {
        sum.wrapping_add(u32::from_be_bytes(word.try_into().unwrap()))
    });
    data[record..record + 4].copy_from_slice(b"kern");
    data[record + 4..record + 8].copy_from_slice(&checksum.to_be_bytes());
    data[record + 8..record + 12].copy_from_slice(&offset.to_be_bytes());
    data[record + 12..record + 16].copy_from_slice(&24_u32.to_be_bytes());
    data.extend_from_slice(&table);
    let mut records: Vec<_> = data[12..12 + 16 * count]
        .chunks_exact(16)
        .map(|record| <[u8; 16]>::try_from(record).unwrap())
        .collect();
    records.sort_by_key(|record| <[u8; 4]>::try_from(&record[..4]).unwrap());
    for (slot, record) in data[12..12 + 16 * count].chunks_exact_mut(16).zip(records) {
        slot.copy_from_slice(&record);
    }
    let head = (0..count)
        .map(|i| 12 + 16 * i)
        .find(|&at| &data[at..at + 4] == b"head")
        .unwrap();
    let head_offset = usize::try_from(u32::from_be_bytes(
        data[head + 8..head + 12].try_into().unwrap(),
    ))
    .unwrap();
    data[head_offset + 8..head_offset + 12].fill(0);
    let checksum = data.chunks_exact(4).fold(0_u32, |sum, word| {
        sum.wrapping_add(u32::from_be_bytes(word.try_into().unwrap()))
    });
    data[head_offset + 8..head_offset + 12]
        .copy_from_slice(&0xB1B0_AFBA_u32.wrapping_sub(checksum).to_be_bytes());
    let font = fmd_font::Font::parse(data).unwrap();
    assert_eq!(font.kerning('A', 'V'), -100, "the actual custom font kern");
    let delta = 100.0 / f64::from(font.units_per_em);
    let custom = Engine::new(fmd_math::faces::FaceSet::from_fonts(vec![font]));
    let reference = fmd_math::MacroSet::pack("default").unwrap();
    let natural = custom.typeset_text("AV").unwrap();
    let controlled = custom.typeset_text_with_macros("AV", &reference).unwrap();
    assert!(
        (controlled.width - natural.width - delta).abs() < EPS,
        "default font control must remove the custom kern: {} vs {}, delta {delta}",
        controlled.width,
        natural.width
    );
}

#[test]
fn size_declarations_use_latex_math_sizes_for_nested_scripts() {
    // Measured with article[10pt] + amsmath/amssymb, declaring the size
    // outside math: {\large $x^{a^b}$} and {\small $x^{a^b}$}.
    // LaTeX ignores a size command inside already-open math. TexText's
    // surrounding size declaration is the valid oracle for these cases.
    let e = engine();
    for (source, sizes, width, height, shifts) in [
        (
            r"{\large $x^{a^b}$}",
            [1.2, 0.8, 0.6],
            1.547_803,
            1.134_471,
            [0.435_47, 0.717_803],
        ),
        (
            r"{\small $x^{a^b}$}",
            [0.9, 0.6, 0.5],
            1.345_901,
            0.981_331,
            [0.382_329, 0.634_109],
        ),
    ] {
        let layout = e.typeset_text(source).expect(source);
        for (ch, expected) in ['x', 'a', 'b'].into_iter().zip(sizes) {
            assert!(
                (glyph_size(&layout, ch) - expected).abs() < EPS,
                "{source}, {ch}: size {} vs {expected}",
                glyph_size(&layout, ch)
            );
        }
        assert!(
            (layout.width - width).abs() < 1e-5,
            "{source}: width {} vs {width}",
            layout.width
        );
        assert!(
            (layout.height - height).abs() < 1e-5,
            "{source}: height {} vs {height}",
            layout.height
        );
        for (ch, expected) in ['a', 'b'].into_iter().zip(shifts) {
            assert!(
                (glyph_y(&layout, ch) - expected).abs() < 1e-5,
                "{source}: {ch}"
            );
        }
    }
}

#[test]
fn declared_sizes_keep_texs_script_delimiter_and_group_drops() {
    // Real showbox, article[10pt] + amsmath/amssymb. The large script
    // delimiter walks up to cmr12's small variant; normal uses cmr10.
    let e = engine();
    for (source, width, height, depth) in [
        (r"$x^{\left(\frac{1}{3}\right)}$", 1.979_59, 1.107_639, 0.0),
        (
            r"{\small $x^{\left(\frac{1}{3}\right)}$}",
            1.874_026,
            0.996_873,
            0.0,
        ),
        (
            r"{\large $x^{\left(\frac{1}{3}\right)}$}",
            2.238_243,
            1.329_166,
            0.0,
        ),
        (r"{\large ${ (a) }^2_3$}", 2.005_684, 1.098_889, 0.35),
    ] {
        let l = e.typeset_text(source).expect(source);
        assert!(
            (l.width - width).abs() < 1e-5
                && (l.height - height).abs() < 1e-5
                && (l.depth - depth).abs() < 1e-5,
            "{source}: ({}, {}, {}) vs ({width}, {height}, {depth})",
            l.width,
            l.height,
            l.depth
        );
    }
}

#[test]
fn normal_math_keeps_the_measured_ten_seven_five_box() {
    // The same real TeX probe at normalsize. Optical-size corrections
    // under declarations must preserve the default 10/7/5 family.
    let layout = engine().typeset("x^{a^b}", Style::Text).unwrap();
    assert!(
        (layout.width - 1.421_959).abs() < 1e-5,
        "width {}",
        layout.width
    );
    assert!(
        (layout.height - 1.011_893).abs() < 1e-5,
        "height {}",
        layout.height
    );
    for (ch, expected) in [('x', 1.0), ('a', 0.7), ('b', 0.5)] {
        assert!((glyph_size(&layout, ch) - expected).abs() < EPS);
    }
}

#[test]
fn declarations_use_optical_metrics_for_accents_and_primes() {
    // Real TeX showbox, valid declarations outside the math island.
    let e = engine();
    let mut mismatches = Vec::new();
    for (source, width, height) in [
        (r"{\small $x^{\vec{a}}$}", 0.979_234, 0.814_496),
        (r"{\small $x'$}", 0.793_748, 0.715_663),
        (r"{\small $x^{\hat{a}}$}", 0.979_234, 0.798_996),
        (r"{\large $x^{\vec{a}}$}", 1.169_191, 1.009_027),
        (r"{\large $x'$}", 0.948_259, 0.879_916),
        (r"{\large $x^{\hat{a}}$}", 1.169_191, 0.991_026),
    ] {
        let l = e.typeset_text(source).expect(source);
        if (l.width - width).abs() >= 1e-5 || (l.height - height).abs() >= 1e-5 {
            mismatches.push(format!(
                "{source}: ({}, {}) vs ({width}, {height})",
                l.width, l.height
            ));
        }
    }
    assert!(mismatches.is_empty(), "{}", mismatches.join("\n"));
}

#[test]
fn display_fraction_shifts_are_num1_denom1() {
    // 'a' and 'x' are short enough that rule 15d's clearances do not bind:
    // the shifts are exactly σ8 and σ11.
    let l = engine().typeset(r"\frac{a}{x}", Style::Display).unwrap();
    assert!(
        (glyph_y(&l, 'a') - CM.num1).abs() < EPS,
        "{}",
        glyph_y(&l, 'a')
    );
    assert!(
        (glyph_y(&l, 'x') - (-CM.denom1)).abs() < EPS,
        "{}",
        glyph_y(&l, 'x')
    );
    // The bar is θ thick, centered on the axis.
    let bar = &l.rules[0];
    assert!((bar.y - (CM.axis_height - CM.rule_thickness / 2.0)).abs() < EPS);
    assert!((bar.height - CM.rule_thickness).abs() < EPS);
}

#[test]
fn text_fraction_shifts_are_num2_denom2() {
    let l = engine().typeset(r"\frac{a}{x}", Style::Text).unwrap();
    assert!((glyph_y(&l, 'a') - CM.num2).abs() < EPS);
    assert!((glyph_y(&l, 'x') - (-CM.denom2)).abs() < EPS);
    // Interiors are script-size.
    assert!((glyph_size(&l, 'a') - 0.7).abs() < EPS);
}

#[test]
fn display_fraction_interiors_are_full_size() {
    let l = engine().typeset(r"\frac{a}{x}", Style::Display).unwrap();
    assert!((glyph_size(&l, 'a') - 1.0).abs() < EPS);
}

#[test]
fn deep_numerator_binds_the_clearance() {
    // 'y' has a descender: in text style rule 15d's numerator clearance
    // binds (u − depth(y·0.7) − (axis + θ/2) < θ) and u grows past σ9.
    let l = engine().typeset(r"\frac{y}{x}", Style::Text).unwrap();
    assert!(glyph_y(&l, 'y') > CM.num2 + EPS, "{}", glyph_y(&l, 'y'));
}

#[test]
fn superscript_shift_is_sup1_in_display() {
    let l = engine().typeset(r"x^2", Style::Display).unwrap();
    assert!((glyph_y(&l, '2') - CM.sup1).abs() < EPS);
    assert!((glyph_size(&l, '2') - 0.7).abs() < EPS);
}

#[test]
fn superscript_shift_is_sup2_in_text() {
    let l = engine().typeset(r"x^2", Style::Text).unwrap();
    assert!((glyph_y(&l, '2') - CM.sup2).abs() < EPS);
}

#[test]
fn cramped_superscript_uses_sup3() {
    // Inside a radicand the context is cramped: σ15.
    let l = engine().typeset(r"\sqrt{x^2}", Style::Display).unwrap();
    assert!((glyph_y(&l, '2') - CM.sup3).abs() < EPS);
}

#[test]
fn lone_subscript_shift_is_sub1() {
    let l = engine().typeset(r"x_i", Style::Display).unwrap();
    assert!((glyph_y(&l, 'i') - (-CM.sub1)).abs() < EPS);
}

#[test]
fn simultaneous_scripts_use_sub2_and_separate_by_4_theta() {
    let l = engine().typeset(r"x_i^2", Style::Display).unwrap();
    let sup_y = glyph_y(&l, '2');
    let sub_y = glyph_y(&l, 'i');
    assert!(sup_y >= CM.sup1 - EPS);
    assert!(sub_y <= -CM.sub2 + EPS);
    // The clash rule guarantees at least 4θ between sup bottom and sub top
    // (measure with the actual glyph inks via layout extents).
    assert!(sup_y - sub_y > 4.0 * CM.rule_thickness);
}

#[test]
fn scriptscript_is_half_size() {
    let l = engine().typeset(r"x^{y^2}", Style::Display).unwrap();
    assert!((glyph_size(&l, '2') - 0.5).abs() < EPS);
    assert!((glyph_size(&l, 'y') - 0.7).abs() < EPS);
    assert!((glyph_size(&l, 'x') - 1.0).abs() < EPS);
}

#[test]
fn radical_rule_position_and_thickness() {
    let e = engine();
    let x_alone = e.typeset("x", Style::Display).unwrap();
    let l = e.typeset(r"\sqrt{x}", Style::Display).unwrap();
    // Rule 11: the clearance is at least ψ, growing by half the sign's
    // excess when the natural √ glyph overshoots the target (CM's does
    // over a lone 'x').
    let psi = CM.rule_thickness + 0.25 * CM.x_height;
    let rule = &l.rules[0];
    assert!((rule.height - CM.rule_thickness).abs() < EPS);
    let clearance = rule.y - x_alone.height;
    assert!(clearance >= psi - 1e-9, "clearance {clearance} < ψ {psi}");
    assert!(clearance < psi + 0.25, "excess out of bounds: {clearance}");
}

#[test]
fn radical_index_is_scriptscript_and_raised() {
    let l = engine().typeset(r"\sqrt[3]{x}", Style::Display).unwrap();
    assert!((glyph_size(&l, '3') - 0.5).abs() < EPS);
    assert!(glyph_y(&l, '3') > 0.3, "degree must be raised");
}

fn glyph_x(layout: &Layout, ch: char) -> f64 {
    layout
        .glyphs
        .iter()
        .find(|g| g.ch == ch)
        .unwrap_or_else(|| panic!("glyph {ch} in {layout:?}"))
        .x
}

/// franken_manim fm-5wq.56: the text face carries every ligature glyph, so
/// text mode typesets the curly quotes and dashes instead of refusing them.
#[test]
fn text_ligature_glyphs_typeset_from_the_bundled_text_face() {
    let e = engine();
    for (source, ch) in [
        ("can't", '\u{2019}'),
        ("`a", '\u{2018}'),
        ("``a''", '\u{201C}'),
        ("``a''", '\u{201D}'),
        ("1--2", '\u{2013}'),
        ("a---b", '\u{2014}'),
    ] {
        let layout = e.typeset_text(source).expect(source);
        assert!(
            layout.glyphs.iter().any(|glyph| glyph.ch == ch),
            "{source}: no {ch:?} glyph"
        );
        assert!(
            !layout
                .glyphs
                .iter()
                .any(|glyph| glyph.ch == '\'' || glyph.ch == '`')
        );
    }
}

#[test]
fn radical_degree_follows_plain_tex_root() {
    // plain.tex's \r@@t: \mkern5mu \raise.6\dimen@ \copy\rootbox \mkern-10mu
    // \box0, with \dimen@ = \ht0 - \dp0 of the radical box. In display
    // style one mu is cmsy10's quad over 18 (§716).
    let e = engine();
    let plain = e.typeset(r"\sqrt{x}", Style::Display).unwrap();
    let rooted = e.typeset(r"\sqrt[3]{x}", Style::Display).unwrap();
    let raise = 0.6 * (plain.height - plain.depth);
    assert!(
        (glyph_y(&rooted, '3') - raise).abs() < EPS,
        "degree at {} for .6(ht - dp) = {raise}",
        glyph_y(&rooted, '3')
    );
    let degree = e.typeset("3", Style::ScriptScript).unwrap().width;
    let mu = CM.quad / 18.0;
    let shift = 5.0 * mu + degree - 10.0 * mu;
    assert!(
        (glyph_x(&rooted, 'x') - glyph_x(&plain, 'x') - shift).abs() < EPS,
        "radicand moved {} for 5mu + w - 10mu = {shift}",
        glyph_x(&rooted, 'x') - glyph_x(&plain, 'x')
    );
}

#[test]
fn a_radical_box_encloses_its_sign() {
    // TeX packs the raised sign with the overbar (§737), so the box is as
    // deep as the sign's descent below the radicand, not just the radicand.
    let e = engine();
    let l = e.typeset(r"\sqrt{x}", Style::Display).unwrap();
    let contours = fmd_math::paths::resolve_paths(&e, &l).unwrap();
    let ink_bottom = contours
        .iter()
        .flat_map(|contour| {
            std::iter::once(contour.start).chain(contour.segments.iter().map(|segment| {
                match *segment {
                    fmd_math::PathSeg::Line { to } | fmd_math::PathSeg::Quad { to, .. } => to,
                }
            }))
        })
        .map(|(_, y)| y)
        .fold(f64::INFINITY, f64::min);
    assert!(ink_bottom < 0.0, "the sign descends below the baseline");
    assert!(
        -l.depth <= ink_bottom + 1e-9,
        "depth {} leaves ink at {ink_bottom} outside the box",
        l.depth
    );
}

#[test]
fn big_op_is_display_scaled_and_axis_centered() {
    let e = engine();
    let display = e.typeset(r"\sum", Style::Display).unwrap();
    let text = e.typeset(r"\sum", Style::Text).unwrap();
    // cmex10's display summation (char 0x58) and its text one (0x50) are two
    // designed glyphs, not one scaled; their TFM boxes are 1.6 and 1.0 em.
    let ds = display.height + display.depth;
    let ts = text.height + text.depth;
    assert!(
        (ds - 1.6).abs() < 1e-4 && (ts - 1.0).abs() < 1e-4,
        "{ds} {ts}"
    );
    assert_eq!(display.glyphs[0].face, fmd_math::faces::FACE_MATH_EXTENSION);
    assert_ne!(display.glyphs[0].gid, text.glyphs[0].gid);
    // Axis-centered (tex.web §749): the box's center at σ22.
    for layout in [&display, &text] {
        let center = (layout.height - layout.depth) / 2.0;
        assert!((center - CM.axis_height).abs() < 1e-9, "{center}");
    }
}

#[test]
fn display_integrals_take_the_cmex_display_size() {
    // cmex10's display integral is twice the text one (2.222 em vs 1.111 em);
    // the `\sum` class keeps its 1.4.
    let e = engine();
    for op in [r"\int", r"\oint", r"\iint"] {
        let display = e.typeset(op, Style::Display).unwrap();
        // amsmath builds `\iint` from two `\int`s; cmex has no single
        // character, so it keeps the fallback face's glyph at the display
        // ratio (the glyph assertion below is for `\int` and `\oint`).
        let text = e.typeset(op, Style::Text).unwrap();
        // The designed display glyph (cmex 0x5A, 0x49) against the text one
        // (0x52, 0x48): ink 2.222 em against 1.111 em.
        let ratio = (display.height + display.depth) / (text.height + text.depth);
        assert!(
            (ratio - CM.display_integral_scale).abs() < 2e-3,
            "{op}: {ratio}"
        );
        assert!(
            op == r"\iint"
                || display
                    .glyphs
                    .iter()
                    .all(|g| g.face == fmd_math::faces::FACE_MATH_EXTENSION
                        && (g.size - 1.0).abs() < EPS),
            "{op}: cmex's own display glyph, unscaled"
        );
    }
    assert!((CM.display_integral_scale - 2.0).abs() < EPS);
}

#[test]
fn display_limits_go_above_and_below_with_the_xi_gaps() {
    let e = engine();
    let l = e.typeset(r"\sum_{n=1}^{N}", Style::Display).unwrap();
    // Upper limit above the op, lower below.
    let n_upper = glyph_y(&l, 'N');
    let one = glyph_y(&l, '1');
    assert!(n_upper > 0.5, "upper limit above: {n_upper}");
    assert!(one < -0.5, "lower limit below: {one}");
    // In text style the same scripts sit beside the operator.
    let t = e.typeset(r"\sum_{n=1}^{N}", Style::Text).unwrap();
    assert!((glyph_y(&t, 'N') - CM.sup2).abs() < 0.2);
}

/// Rule 18a reads sup_drop and sub_drop from the symbol font of the size the
/// scripts are set at (tex.web §756): cmsy7's 0.353171 × 7 pt and
/// 0.071428 × 7 pt, not cmsy10's at the base's size. Each value is real
/// TeX's `\showbox` under `article` + `amsmath` (franken_manim fm-4ptw), in
/// ems of 10 pt: where the drop binds, the scripts sit exactly there.
#[test]
fn script_drops_are_read_at_the_script_size() {
    let e = engine();
    // Display integral with limits: the superscript 11.13903 pt up and δ =
    // 4.44444 pt right of the subscript, which is 9.11122 pt down.
    let int = e.typeset(r"\int_0^1 x", Style::Display).unwrap();
    let (sup, sub) = (glyph_y(&int, '1'), glyph_y(&int, '0'));
    assert!((sup - 1.113_903).abs() < 1e-4, "\\int superscript at {sup}");
    assert!((sub + 0.911_122).abs() < 1e-4, "\\int subscript at {sub}");
    let delta = glyph_x(&int, '1') - glyph_x(&int, '0');
    assert!((delta - 0.444_444).abs() < 1e-4, "\\int δ {delta}");
    // A group nucleus: 7.5 pt tall, so its superscript sits 7.5 - 2.47220 =
    // 5.0278 pt up; 2.5 pt deep, so its subscript sits 2.5 + 0.5 = 3.0 pt down.
    let sup = glyph_y(&e.typeset(r"{(a)}^2", Style::Display).unwrap(), '2');
    assert!((sup - 0.502_78).abs() < 1e-4, "{{(a)}}^2 at {sup}");
    let sub = glyph_y(&e.typeset(r"{(a)}_2", Style::Text).unwrap(), '2');
    assert!((sub + 0.3).abs() < 1e-4, "{{(a)}}_2 at {sub}");
}

/// Script styles read Appendix G's parameters from the fonts loaded at their
/// own size: cmsy7 and cmex7 in script style, cmsy5 and cmex7 at 5 pt in
/// scriptscript style, never cmsy10 and cmex10 scaled down (tex.web's
/// cur_size; franken_manim fm-giir). Each value is real TeX's `\showbox`
/// under `article` + `amsmath`, in ems of 10 pt. TeX truncates every scaled
/// fontdimen to a whole sp, so the exact products agree within a few sp.
#[test]
fn script_styles_read_their_own_sizes_fontdimens() {
    let e = engine();
    let close = |ours: f64, tex_pt: f64| (ours - tex_pt / 10.0).abs() < 1e-5;
    // e^{-x^2}: the outer script rises sup2 of cmsy10 (3.62892 pt), the
    // inner one sup2 of cmsy7 (3.01779 pt; cmsy10's scaled would be 2.54).
    let y = glyph_y(&e.typeset(r"e^{-x^2}", Style::Text).unwrap(), '2');
    assert!(close(y, 3.62892 + 3.01779), "e^{{-x^2}}: 2 at {y}");
    // x^{a\,b}: 3 mu in script style is 3/18 of cmsy7's quad, 1.36574 pt.
    let width = |s: &str| e.typeset(s, Style::Text).unwrap().width;
    let thin = width(r"x^{a\,b}") - width("x^{ab}");
    assert!(close(thin, 15.43532 - 14.06958), "thin space {thin}");
    // \quad and `\ ` are text-font glue, not mu: cmr10's quad and interword
    // space at 10 pt even in a script.
    let quad = width(r"x^{a\quad b}") - width("x^{ab}");
    assert!(close(quad, 24.06714 - 14.06958), "script quad {quad}");
    let space = width(r"x^{a\ b}") - width("x^{ab}");
    assert!(
        close(space, 17.40210 - 14.06958),
        "script control space {space}"
    );
    // x^{\frac12}: the bar is ξ8 of cmex7, 0.33998 pt, on cmsy7's axis;
    // num2 and denom2 come from cmsy7 too (rule 15b's clearances do not
    // bind for 1 over 2).
    let frac = e.typeset(r"x^{\frac12}", Style::Text).unwrap();
    let bar = &frac.rules[0];
    assert!(close(bar.height, 0.33998), "bar {bar:?}");
    assert!(close(bar.y, 3.62892 + 1.58002), "bar {bar:?}");
    assert!(close(glyph_y(&frac, '1'), 3.62892 + 2.68732), "{frac:?}");
    assert!(close(glyph_y(&frac, '2'), 3.62892 - 2.4095), "{frac:?}");
    // x^{a^{b^c}}: the third level stays in scriptscript, rising sup2 of
    // cmsy5 (2.01776 pt).
    let tower = e.typeset(r"x^{a^{b^c}}", Style::Text).unwrap();
    let c = glyph_y(&tower, 'c');
    assert!(close(c, 3.62892 + 3.01779 + 2.01776), "c at {c}");
    // {(a)}^2_3 set in scriptscript: sup2 and sub2 of cmsy5, then the
    // subscript lowered to rule 18e's 4θ gap, θ = ξ8 of cmex7 at 5 pt.
    let both = e
        .typeset(r"\scriptscriptstyle{(a)}^2_3", Style::Text)
        .unwrap();
    let (sup, sub) = (glyph_y(&both, '2'), glyph_y(&both, '3'));
    assert!(close(sup, 3.22221 + 0.97137 - 2.17583), "sup at {sup}");
    assert!(close(sub, -2.17583), "sub at {sub}");
}

/// franken_manim fm-shqq: LaTeX's `\cong` is `\mathpalette\@vereq\sim`,
/// cmsy's ∼ over cmr's =. The rows sit 0.5 pt closer than touching, and the
/// stack is lowered 0.5 pt. TeX's `\showbox` (TeX Live 2025, article +
/// amsmath):
/// - text: 7.7778 × (6.3375 + 0.5) pt;
/// - script: 6.25002 × (4.3771 + 0.5);
/// - scriptscript: 5.41673 × (2.96405 + 0.5).
#[test]
fn cong_is_latexs_vereq_stack() {
    let e = engine();
    let close = |ours: f64, tex_pt: f64| (ours - tex_pt / 10.0).abs() < 1e-5;
    for (tex, w, h) in [
        (r"\cong", 7.7778, 6.3375),
        (r"\scriptstyle\cong", 6.25002, 4.3771),
        (r"\scriptscriptstyle\cong", 5.41673, 2.96405),
    ] {
        let l = e.typeset(tex, Style::Text).unwrap();
        assert!(
            close(l.width, w) && close(l.height, h) && close(l.depth, 0.5),
            "{tex}: {l:?}"
        );
        // Two Computer Modern glyphs, never a fallback face's ≅.
        let chars: Vec<char> = l.glyphs.iter().map(|g| g.ch).collect();
        assert_eq!(chars, ['∼', '='], "{tex}");
    }
    // Text style: ∼'s baseline is 3.66875 - 0.5 - 0.5 pt up and ='s 0.5 pt
    // down; both are 0.777779 em wide, so neither is shifted.
    let l = e.typeset(r"\cong", Style::Text).unwrap();
    assert!(close(glyph_y(&l, '∼'), 2.66875), "{l:?}");
    assert!(close(glyph_y(&l, '='), -0.5), "{l:?}");
    assert!(close(glyph_x(&l, '=') - glyph_x(&l, '∼'), 0.0), "{l:?}");
    // Script style: cmr7's = is narrower than cmsy7's ∼ and is centred
    // (TeX's glue set 0.05556fil on each side).
    let l = e.typeset(r"\scriptstyle\cong", Style::Text).unwrap();
    assert!(close(glyph_x(&l, '=') - glyph_x(&l, '∼'), 0.05556), "{l:?}");
    assert!(close(glyph_y(&l, '∼'), 2.68855 - 1.0), "{l:?}");
    // A relation: thick spaces on either side. TeX truncates the mu glue to
    // 2.77771 pt rather than 5/18 of the quad, hence 1e-4 em here.
    let w = e.typeset(r"a\cong b", Style::Text).unwrap().width;
    assert!((w - 2.291077).abs() < 1e-4, "a\\cong b: {w}");
}

/// fontmath.ltx and amsmath build the long arrows from shorter relations
/// (franken_manim fm-fm-long-arrows-joinrel-r11-b2c5): `\longrightarrow` is
/// `\relbar\joinrel\rightarrow`, a smashed cmsy minus, \joinrel's -3 mu and
/// cmsy's arrow. TeX's boxes under the Reference's preamble, in pt, with
/// the glyphs in order: two CM pieces where the Reference has two, never a
/// fallback face's single arrow.
#[test]
fn long_arrows_join_cm_pieces_as_amsmath_does() {
    let e = engine();
    let close = |ours: f64, tex_pt: f64| (ours - tex_pt / 10.0).abs() < 1e-4;
    for (tex, w, h, d, glyphs) in [
        (r"\longrightarrow", 16.11119, 3.66875, 0.0, &['−', '→'][..]),
        (r"\Longrightarrow", 16.11119, 3.66875, 0.0, &['=', '⇒']),
        (r"\longleftarrow", 16.11119, 3.66875, 0.0, &['←', '−']),
        (r"\Longleftarrow", 16.11119, 3.66875, 0.0, &['⇐', '=']),
        (r"\longleftrightarrow", 18.3334, 3.66875, 0.0, &['←', '→']),
        (r"\Longleftrightarrow", 18.3334, 3.66875, 0.0, &['⇐', '⇒']),
        (r"\mapsto", 10.00002, 3.66875, 0.0, &['↦', '→']),
        (r"\longmapsto", 16.11119, 3.66875, 0.0, &['⟼', '−', '→']),
        (r"\hookrightarrow", 11.11118, 4.63747, 0.0, &['↪', '→']),
        (r"\hookleftarrow", 11.11118, 4.63747, 0.0, &['←', '↩']),
        (
            r"\scriptstyle\longrightarrow",
            12.85652,
            2.68855,
            0.0,
            &['−', '→'],
        ),
        (
            r"\scriptstyle\Longrightarrow",
            12.74542,
            2.68855,
            0.0,
            &['=', '⇒'],
        ),
        (r"\scriptstyle\mapsto", 7.97224, 2.68855, 0.0, &['↦', '→']),
        (
            r"\scriptstyle\hookrightarrow",
            8.9815,
            3.45709,
            0.0,
            &['↪', '→'],
        ),
        (
            r"\scriptscriptstyle\longrightarrow",
            10.99553,
            1.98203,
            0.0,
            &['−', '→'],
        ),
        (
            r"\scriptscriptstyle\hookrightarrow",
            7.87048,
            2.57402,
            0.07402,
            &['↪', '→'],
        ),
    ] {
        let l = e.typeset(tex, Style::Text).unwrap();
        assert!(
            close(l.width, w) && close(l.height, h) && close(l.depth, d),
            "{tex}: {} x ({} + {}) against TeX's {w} x ({h} + {d}) pt",
            l.width,
            l.height,
            l.depth
        );
        let chars: Vec<char> = l.glyphs.iter().map(|g| g.ch).collect();
        assert_eq!(chars, glyphs, "{tex}");
    }
    // The arrow starts at the minus's width less 3 mu: 7.7778 - 1.66663 pt.
    let l = e.typeset(r"\longrightarrow", Style::Text).unwrap();
    assert!(close(glyph_x(&l, '→') - glyph_x(&l, '−'), 6.11117), "{l:?}");
    // One relation, with thick spaces on either side and limits over it.
    for (tex, w, h) in [
        (r"a\longrightarrow b", 31.24416, 6.94444),
        (r"a\overset{f}{\longrightarrow}b", 31.24416, 12.00206),
    ] {
        let l = e.typeset(tex, Style::Display).unwrap();
        assert!(close(l.width, w) && close(l.height, h), "{tex}: {l:?}");
    }
}

/// amsmath's dots read the token after them (franken_manim
/// fm-fm-amsmath-dots-context-r11-ymdd). `\dots` is centred before a binary
/// operator or relation and adds `\,` before a closing delimiter; `\cdots`
/// and `\dotso` add it before a comma, semicolon, period or closing
/// delimiter too, `\dotsc` not before a comma. TeX's widths in display
/// style under the Reference's preamble, in pt.
#[test]
fn amsmath_dots_read_the_next_token() {
    let e = engine();
    for (tex, tex_pt, dot, count) in [
        (r"(a\dots)", 28.06355, '.', 3),
        (r"(a\ldots)", 26.39693, '.', 3),
        (r"\left(a\dots\right)", 28.06355, '.', 3),
        (r"\{a\dots\}", 30.28578, '.', 3),
        (r"\langle a\dots\rangle", 28.06355, '.', 3),
        (r"a,\dots,b", 31.7996, '.', 3),
        (r"a\dots b", 24.57741, '.', 3),
        (r"a\dots.", 23.06354, '.', 4),
        (r"a+\dots+b", 45.68843, '⋅', 3),
        (r"a\dots\le b", 36.244, '⋅', 3),
        (r"a\dots\neq b", 36.244, '⋅', 3),
        (r"a\cdots b", 24.57741, '⋅', 3),
        (r"a\cdots,b", 30.68845, '⋅', 3),
        (r"a\cdots.", 24.73016, '⋅', 3),
        (r"a\ldots,b", 29.02182, '.', 3),
        (r"a\dotso,b", 30.68845, '.', 3),
        (r"a\dotsc;b", 30.68845, '.', 3),
        (r"a\dotsc,b", 29.02182, '.', 3),
    ] {
        let l = e.typeset(tex, Style::Display).unwrap();
        assert!(
            (l.width - tex_pt / 10.0).abs() < 1e-4,
            "{tex}: {} em against TeX's {tex_pt} pt",
            l.width
        );
        let dots = l.glyphs.iter().filter(|g| g.ch == dot).count();
        assert_eq!(dots, count, "{tex}: {count} {dot:?} in {l:?}");
    }
    // At the end of inline math the closing `$` is the token after: TeX
    // boxes `$a,\dots$` at 23.06354 pt and `$a,\ldots$` at 21.39691 pt.
    for (source, tex_pt) in [(r"$a,\dots$", 23.06354), (r"$a,\ldots$", 21.39691)] {
        let width = e.typeset_text(source).unwrap().width;
        assert!(
            (width - tex_pt / 10.0).abs() < 1e-4,
            "{source}: {width} em against TeX's {tex_pt} pt"
        );
    }
}

/// The Reference's template loads physics, whose default `trig` option gives
/// the trig functions, `\log`, `\ln`, `\exp`, `\det` and `\Pr` argument forms
/// (franken_manim fm-fm-physics-trig-parens-r11-nou8): `\cos(x)` is
/// `\cos\mathopen{}\mathclose{\left(x\right)}`, `\cos[2](x)` raises the 2,
/// and `\exp{x}` sets visible braces. The default pack carries them; the
/// empty pack keeps plain LaTeX. TeX's display boxes under each preamble, pt.
#[test]
fn physics_function_arguments_follow_the_default_pack() {
    use fmd_math::MacroSet;
    let e = engine();
    let close = |ours: f64, tex_pt: f64| (ours - tex_pt / 10.0).abs() < 1e-4;
    let default = MacroSet::pack("fmd-math/pack/default").unwrap();
    for (tex, w, h, d) in [
        (r"\cos(x)", 26.88197, 7.5, 2.5),
        (r"\cos (x)", 26.88197, 7.5, 2.5),
        (r"\cos(x)y", 32.14357, 7.5, 2.5),
        (r"\cos(x)(y)", 39.92137, 7.5, 2.5),
        (r"\cos(x),", 29.65976, 7.5, 2.5),
        (r"\cos(x)+1", 44.10413, 7.5, 2.5),
        (r"a\cos(x)", 33.83449, 7.5, 2.5),
        (r"\cos((a)b)", 38.52205, 7.5, 2.5),
        (r"\cos(\frac{a}{b})", 33.01924, 11.50008, 6.85951),
        (r"\cos[2](x)", 31.3681, 8.64003, 2.5),
        (r"\cos[2] (x)", 31.3681, 8.64003, 2.5),
        (r"\sin[2] x", 28.54866, 7.5, 2.5),
        (r"\sin^2(x)", 30.257, 8.7175, 2.5),
        (r"\log(x)", 26.40976, 7.5, 2.5),
        (r"\exp(x)", 28.77089, 7.5, 2.5),
        (r"\exp{x}", 30.99312, 7.5, 2.5),
        (r"\det(A)", 29.16673, 7.5, 2.5),
        // Inside the argument `\dots` sees `\argclose`, not `)`: no `\,`.
        (r"\sin(\dots)", 31.72221, 7.5, 2.5),
    ] {
        let l = e
            .typeset_with_macros(tex, Style::Display, &default)
            .unwrap();
        assert!(
            close(l.width, w) && close(l.height, h) && close(l.depth, d),
            "{tex}: {} x ({} + {}) against TeX's {w} x ({h} + {d}) pt",
            l.width,
            l.height,
            l.depth
        );
    }
    // Plain LaTeX (article + amsmath): ordinary parentheses and braces.
    let empty = MacroSet::pack("fmd-math/pack/empty").unwrap();
    for (tex, w, h, d) in [
        (r"\cos(\frac{a}{b})", 28.85258, 11.07062, 6.85951),
        (r"\exp{x}", 22.65971, 4.30554, 1.94444),
    ] {
        let l = e.typeset_with_macros(tex, Style::Display, &empty).unwrap();
        assert!(
            close(l.width, w) && close(l.height, h) && close(l.depth, d),
            "{tex} without physics: {l:?}"
        );
    }
    // A piece of a multi-part Tex keeps the plain operator.
    assert!(
        e.typeset_with_macros(r"\cos(", Style::Display, &default)
            .is_ok()
    );
}

/// LaTeX's operator names are `\mathop{\operator@font …}`: the word is a
/// sub-list, so its last character's italic correction (cmr10's `g`,
/// 0.01389 em) stays inside the operator's box. TeX's display widths under
/// article + amsmath, in pt.
#[test]
fn operator_names_keep_their_last_italic_correction() {
    let e = engine();
    for (tex, tex_pt) in [
        (r"\log x", 20.29858),
        (r"\arg z", 20.8125),
        (r"\deg x", 22.5208),
        (r"\lg x", 15.29857),
        (r"\max x", 25.99307),
        (r"\operatorname{erg} x", 20.8819),
    ] {
        let width = e.typeset(tex, Style::Display).unwrap().width;
        assert!(
            (width - tex_pt / 10.0).abs() < 1e-4,
            "{tex}: {width} em against TeX's {tex_pt} pt"
        );
    }
}

/// TeX's class primitives set their argument as one atom of the class.
/// TeX's display widths under article + amsmath, in pt.
#[test]
fn class_primitives_space_their_atom() {
    let e = engine();
    for (tex, tex_pt) in [
        (r"x\mathrel{y}z", 21.62259),
        (r"x\mathbin{y}z", 20.5115),
        (r"x\mathopen{y}z", 16.06717),
        (r"x\mathclose{y}z", 16.06717),
        (r"x\mathpunct{y}z", 17.7338),
        (r"x\mathinner{y}z", 19.40042),
        (r"x\mathord{=}z", 18.58337),
        (r"\mathrel{=}", 7.7778),
    ] {
        let width = e.typeset(tex, Style::Display).unwrap().width;
        assert!(
            (width - tex_pt / 10.0).abs() < 1e-4,
            "{tex}: {width} em against TeX's {tex_pt} pt"
        );
    }
}

/// amsmath's `\overset` and `\underset` take their class from `\binrel@`:
/// the base's spaces against two empty Ords make a relation (thick) or a
/// binary operator (medium), otherwise a braced Ord. The argument's own
/// braces do not hide the class; a second pair does. TeX's widths in
/// display style under the Reference's preamble, in pt.
#[test]
fn overset_classes_follow_amsmaths_binrel() {
    let e = engine();
    for (tex, tex_pt) in [
        (r"x\overset{a}{=}y", 24.31009),
        (r"x\overset{f}{\longrightarrow}y", 32.64348),
        (r"x\underset{a}{\longrightarrow}y", 32.64348),
        (r"x\overset{a}{+}y", 23.199),
        (r"x\overset{a}{{=}}y", 18.75467),
        (r"x\overset{a}{b}y", 15.31451),
        (r"x\overset{a}{b=c}y", 32.9293),
    ] {
        let width = e.typeset(tex, Style::Display).unwrap().width;
        assert!(
            (width - tex_pt / 10.0).abs() < 1e-4,
            "{tex}: {width} em against TeX's {tex_pt} pt"
        );
    }
}

#[test]
fn integrals_take_side_scripts_even_in_display() {
    let l = engine().typeset(r"\int_0^1", Style::Display).unwrap();
    let sum_layout = engine().typeset(r"\sum_0^1", Style::Display).unwrap();
    // The integral's scripts are beside it (its width exceeds the bare
    // glyph), while the sum's limits stack (width equals the op width).
    let int_alone = engine().typeset(r"\int", Style::Display).unwrap();
    let sum_alone = engine().typeset(r"\sum", Style::Display).unwrap();
    assert!(l.width > int_alone.width + 0.1);
    assert!((sum_layout.width - sum_alone.width).abs() < 0.35);
}

#[test]
fn left_right_delimiters_cover_the_rule_19_target() {
    let e = engine();
    let l = e
        .typeset(r"\left(\frac{a}{x}\right)", Style::Display)
        .unwrap();
    let inner = e.typeset(r"\frac{a}{x}", Style::Display).unwrap();
    let delta = (inner.height - CM.axis_height).max(inner.depth + CM.axis_height);
    let target = (2.0 * delta * CM.delimiter_factor).max(2.0 * delta - CM.delimiter_shortfall);
    // A display fraction needs more than the 1.0 em small paren, so TeX
    // takes a cmex10 successor: each paren is cmex's own glyph (no drawn
    // path, no scaling), and the construct still covers the rule-19 target.
    let parens: Vec<_> = l
        .glyphs
        .iter()
        .filter(|g| g.ch == '(' || g.ch == ')')
        .collect();
    assert_eq!(parens.len(), 2, "one cmex glyph per paren");
    assert!(
        parens
            .iter()
            .all(|g| g.face == fmd_math::faces::FACE_MATH_EXTENSION && (g.size - 1.0).abs() < EPS),
        "cmex successors, unscaled"
    );
    assert!(l.paths.is_empty());
    assert!(l.height + l.depth >= target - 1e-6);

    // The small variant is math family 0's own paren, cmr10's: an inline
    // \left(x\right) needs nothing larger.
    let small = e.typeset(r"\left( x \right)", Style::Text).unwrap();
    assert!(
        small
            .glyphs
            .iter()
            .any(|g| g.ch == '(' && g.face == fmd_math::faces::FACE_ROMAN_10),
        "natural glyph kept"
    );
    assert!(small.paths.is_empty());
}

/// franken_manim fm-u5sb: `\underbrace` and `\overbrace` fill with cmex's
/// four brace pieces and two rules, as LaTeX's `\downbracefill` and
/// `\upbracefill` do, never a drawn band. TeX's `\showbox` (TeX Live 2025,
/// article + amsmath):
/// - `\underbrace{a+b}`: 21.79968 × (6.94444 + 8.0333) pt. Its rules are
///   1.89975 pt wide (glue set) and 1.19997 pt high, their bottoms 3 pt
///   below the base's 0.83333 pt depth, less the row.
/// - `\overbrace{1+2+\cdots+n}`: 64.3354 × (13.64441 + 0.83333) pt.
/// - `\underbrace{x}`: 18.00018 × (4.30554 + 7.19997) pt, the four pieces
///   alone (the fills have no width), and the same in script style.
#[test]
fn braces_fill_with_cmex_pieces_and_rules() {
    use fmd_math::faces::FACE_MATH_EXTENSION;
    let e = engine();
    let close = |ours: f64, tex_pt: f64| (ours - tex_pt / 10.0).abs() < 1e-5;
    // TeX truncates mu glue (a medium space is 2.22217 pt, not 5/18 of
    // the quad), so widths across a + get 1e-4 em; heights and depths do not
    // depend on it.
    let close_mu = |ours: f64, tex_pt: f64| (ours - tex_pt / 10.0).abs() < 1e-4;
    let under = e.typeset(r"\underbrace{a+b}", Style::Text).unwrap();
    assert!(
        close_mu(under.width, 21.79968)
            && close(under.height, 6.94444)
            && close(under.depth, 8.0333),
        "{under:?}"
    );
    let pieces = |l: &fmd_math::Layout| {
        l.glyphs
            .iter()
            .filter(|g| g.face == FACE_MATH_EXTENSION)
            .count()
    };
    assert_eq!(pieces(&under), 4, "{under:?}");
    assert!(under.paths.is_empty(), "no drawn band: {under:?}");
    assert_eq!(under.rules.len(), 2, "{under:?}");
    for rule in &under.rules {
        assert!(
            close_mu(rule.width, 1.89975) && close(rule.height, 1.19997),
            "{rule:?}"
        );
        assert!(close(rule.y, -(0.83333 + 3.0 + 1.19997)), "{rule:?}");
    }
    let over = e.typeset(r"\overbrace{1+2+\cdots+n}", Style::Text).unwrap();
    assert!(
        close_mu(over.width, 64.3354) && close(over.height, 13.64441) && close(over.depth, 0.83333),
        "{over:?}"
    );
    assert_eq!((pieces(&over), over.rules.len()), (4, 2), "{over:?}");
    for tex in [r"\underbrace{x}", r"\scriptstyle\underbrace{x}"] {
        let l = e.typeset(tex, Style::Text).unwrap();
        assert!(
            close(l.width, 18.00018) && close(l.height, 4.30554) && close(l.depth, 7.19997),
            "{tex}: {l:?}"
        );
        assert_eq!((pieces(&l), l.rules.len()), (4, 0), "{tex}: {l:?}");
    }
}

/// franken_manim fm-u5sb: a `\left`/`\right` delimiter that needs nothing
/// larger is TeX's small variant, from math family 0 for parentheses and
/// brackets: cmr10 at text size and cmr7 in a script, never CM Unicode,
/// whose brackets carry 1.48 times cmr10's ink. TeX's `\showbox` (TeX Live
/// 2025, article + amsmath):
/// - `\left[ x \right]` is 11.27084 × (7.5 + 2.5) pt, from `\OT1/cmr/m/n/10 [`;
/// - `\left( x \right)` is 13.49307 pt wide;
/// - `x^{\left[ y \right]}` sets `\OT1/cmr/m/n/7 [`.
#[test]
fn small_left_right_delimiters_are_math_family_zero() {
    use fmd_math::faces::{FACE_ROMAN_7, FACE_ROMAN_10};
    let e = engine();
    let close = |ours: f64, tex_pt: f64| (ours - tex_pt / 10.0).abs() < 1e-5;
    let brackets = e.typeset(r"\left[ x \right]", Style::Text).unwrap();
    assert!(
        close(brackets.width, 11.27084)
            && close(brackets.height, 7.5)
            && close(brackets.depth, 2.5),
        "{brackets:?}"
    );
    for ch in ['[', ']'] {
        let g = brackets.glyphs.iter().find(|g| g.ch == ch).unwrap();
        assert_eq!(g.face, FACE_ROMAN_10, "{ch}");
    }
    let parens = e.typeset(r"\left( x \right)", Style::Text).unwrap();
    assert!(close(parens.width, 13.49307), "{parens:?}");
    let script = e.typeset(r"x^{\left[ y \right]}", Style::Text).unwrap();
    for ch in ['[', ']'] {
        let g = script.glyphs.iter().find(|g| g.ch == ch).unwrap();
        assert_eq!(g.face, FACE_ROMAN_7, "script {ch}");
    }
}

/// The vertical ink extent of the glyphs `keep` selects, ems.
fn glyph_ink_extent(
    e: &Engine,
    l: &Layout,
    keep: impl Fn(&fmd_math::PlacedGlyph) -> bool,
) -> (f64, f64) {
    let mut lo = f64::INFINITY;
    let mut hi = f64::NEG_INFINITY;
    for g in l.glyphs.iter().filter(|g| keep(g)) {
        let font = e.faces().font(g.face).expect("placed face");
        let bbox = font.glyph_bbox(g.gid).expect("inked glyph");
        let s = g.size / f64::from(font.units_per_em);
        lo = lo.min(g.y + f64::from(bbox[1]) * s);
        hi = hi.max(g.y + f64::from(bbox[3]) * s);
    }
    (lo, hi)
}

#[test]
fn delimiters_take_tex_variant_sizes() {
    // Real TeX (LaTeX with amsmath, 10 pt) sets each of these delimiters at
    // a cmex10 variant or extensible size, which here also bounds the box:
    // height = axis + total / 2, depth = total / 2 - axis. The totals in ems
    // are the TeX boxes' height plus depth over 10 pt. The axis-centred
    // delimiter tops every box, so the height pins its size; the depth
    // need only cover it, since content can reach lower here than in TeX
    // (the face's script k in \binom dips below the baseline, where
    // cmmi7's k has no depth).
    use Style::{Display, Text};
    let e = engine();
    for (tex, style, total) in [
        (
            r"\left[\begin{array}{c}\quad \\\end{array}\right]",
            Display,
            1.2,
        ),
        (
            r"\left[\begin{array}{c}\quad \\\quad \\\end{array}\right]",
            Display,
            2.4,
        ),
        (
            r"\left[\begin{array}{c}\quad \\\quad \\\quad \\\end{array}\right]",
            Display,
            3.6,
        ),
        (
            r"\left[\begin{array}{c}\quad \\\quad \\\quad \\\quad \\\quad \\\end{array}\right]",
            Display,
            6.0,
        ),
        (r"\begin{pmatrix}1\\2\\3\end{pmatrix}", Display, 3.6),
        (
            r"\left\{\begin{matrix}a\\b\\c\\d\end{matrix}\right.",
            Display,
            4.8,
        ),
        (
            r"\left\lfloor\begin{matrix}a\\b\\c\end{matrix}\right\rfloor",
            Display,
            3.6,
        ),
        (r"\binom{n}{k}", Text, 1.2),
        (r"\binom{n}{k}", Display, 2.4),
        (r"\Big|", Text, 1.8),
    ] {
        let l = e.typeset(tex, style).unwrap();
        let half = l.height - CM.axis_height;
        assert!(
            (half - total / 2.0).abs() < 1e-6 && l.depth + CM.axis_height > total / 2.0 - 1e-6,
            "{tex} in {style:?}: {} + {} against TeX's {total} em",
            l.height,
            l.depth
        );
    }
    // A fraction in parentheses: TeX's 1.8 em variant, cmex's \Big paren
    // (its ink is the TFM box to Type 1 rounding). The box's depth is the
    // fraction's.
    let l = e
        .typeset(r"\left(\frac{a}{x}\right)", Style::Display)
        .unwrap();
    for paren in ['(', ')'] {
        let (lo, hi) = glyph_ink_extent(&e, &l, |g| g.ch == paren);
        assert!((hi - lo - 1.8).abs() < 2e-3, "{paren}: {lo}..{hi}");
        assert!(((hi + lo) / 2.0 - CM.axis_height).abs() < 2e-3);
    }
    // Angles have no extensible recipe: past five rows TeX's angle stays at
    // its largest variant, 3.0 em, while the matrix grows.
    let l = e
        .typeset(
            r"\left\langle\begin{matrix}a\\b\\c\\d\\e\end{matrix}\right\rangle",
            Style::Display,
        )
        .unwrap();
    let (lo, hi) = glyph_ink_extent(&e, &l, |g| g.ch == '⟨');
    assert!((hi - lo - 3.0).abs() < 2e-3, "angle: {lo}..{hi}");
    assert!(l.height + l.depth > 5.0);
}

#[test]
fn delimiters_take_tex_box_widths() {
    // Real TeX's \wd of each box (LaTeX with amsmath, 10 pt, in ems). The
    // content is digits and array columns, whose widths fmd-math already
    // shares with TeX, so each total is the content plus the delimiters'
    // cmex10 widths: `\big`'s 0.4167 em bracket, the extensible bracket's
    // 0.6667, the 2.4 em paren's 0.7361 and so on.
    use Style::{Display, Text};
    let e = engine();
    for (tex, style, width) in [
        (
            r"\left[\begin{array}{cc}3&1\\0&2\end{array}\right]",
            Display,
            4.055563,
        ),
        (
            r"\left[\begin{array}{c}1\end{array}\right]",
            Display,
            2.333339,
        ),
        (
            r"\left[\begin{array}{c}1\\2\\3\end{array}\right]",
            Display,
            2.833339,
        ),
        (r"\begin{pmatrix}1&2\\3&4\end{pmatrix}", Display, 3.472232),
        (
            r"\left\{\begin{array}{c}1\\2\\3\\4\end{array}\right.",
            Display,
            2.508891,
        ),
        (
            r"\left|\begin{array}{c}1\\2\end{array}\right|",
            Display,
            2.166667,
        ),
        (
            r"\left\langle\begin{array}{c}1\\2\end{array}\right\rangle",
            Display,
            3.000005,
        ),
        (
            r"\left\lfloor\begin{array}{c}1\\2\end{array}\right\rfloor",
            Display,
            2.666673,
        ),
        (r"\Big(1\Big)", Text, 1.694447),
    ] {
        let l = e.typeset(tex, style).unwrap();
        assert!(
            (l.width - width).abs() < 1e-4,
            "{tex} in {style:?}: {} against TeX's {width} em",
            l.width
        );
    }
    // The ink faces the content as cmex's does: a 2.4 em bracket's ink ends
    // 0.0148 em (bracketleftbigg's right bearing) before its box does.
    let l = e
        .typeset(
            r"\left[\begin{array}{cc}3&1\\0&2\end{array}\right]",
            Style::Display,
        )
        .unwrap();
    let bracket = l.glyphs.iter().find(|g| g.ch == '[').expect("cmex bracket");
    let font = e.faces().font(bracket.face).expect("placed face");
    let bbox = font.glyph_bbox(bracket.gid).expect("inked glyph");
    let scale = bracket.size / f64::from(font.units_per_em);
    let (x0, x1) = (
        bracket.x + f64::from(bbox[0]) * scale,
        bracket.x + f64::from(bbox[2]) * scale,
    );
    assert!((0.527781 - x1 - 0.014781).abs() < 1e-6, "ink {x0}..{x1}");
}

#[test]
fn big_operators_take_tex_box_widths() {
    // Real TeX's \wd (LaTeX with amsmath, 10 pt, in ems) of each operator
    // alone: cmex10's TFM width plus, for the integrals, the italic
    // correction TeX's clean_box keeps (display \int: 0.5556 + 0.4444).
    use Style::{Display, Text};
    let e = engine();
    for (tex, style, width) in [
        (r"\int", Display, 1.000002),
        (r"\int", Text, 0.666667),
        (r"\oint", Display, 1.000002),
        (r"\sum", Display, 1.444447),
        (r"\sum", Text, 1.055559),
        (r"\prod", Display, 1.277780),
        (r"\bigcup", Display, 1.111115),
        (r"\bigoplus", Display, 1.511116),
    ] {
        let l = e.typeset(tex, style).unwrap();
        assert!(
            (l.width - width).abs() < 1e-4,
            "{tex} in {style:?}: {} against TeX's {width} em",
            l.width
        );
    }
    // What follows a lone operator starts where TeX's does: the box, then
    // the Op-Ord thin space (\int x is 1.0 + 0.1667 + x in TeX).
    let l = e.typeset(r"\int x", Style::Display).unwrap();
    let x = l.glyphs.iter().find(|g| g.ch != '∫').unwrap();
    assert!((x.x - (1.000002 + 1.0 / 6.0)).abs() < 1e-4, "x at {}", x.x);
    // Scripts sit where rule 13 puts them on a nolimits operator: the
    // subscript at the TFM width, the superscript the italic correction
    // further right (display \int: 0.5556 and 1.0).
    let l = e.typeset(r"\int_0^1", Style::Display).unwrap();
    let at = |ch: char| l.glyphs.iter().find(|g| g.ch == ch).unwrap().x;
    assert!((at('0') - 0.555557).abs() < 1e-4, "sub at {}", at('0'));
    assert!((at('1') - 1.000003).abs() < 1e-4, "sup at {}", at('1'));
}

#[test]
fn null_delimiter_occupies_nulldelimiterspace() {
    let e = engine();
    let with_null = e.typeset(r"\left. x \right.", Style::Display).unwrap();
    let bare = e.typeset(r"x", Style::Display).unwrap();
    assert!((with_null.width - (bare.width + 2.0 * CM.null_delimiter_space)).abs() < 1e-9);
}

#[test]
fn fractions_without_delimiters_carry_null_delimiters() {
    // Rule 15e: TeX wraps every generalized fraction in its delimiters, and
    // \frac's are null: \nulldelimiterspace each side of the bar.
    let e = engine();
    let frac = e.typeset(r"\frac{a}{b}", Style::Display).unwrap();
    let bar = &frac.rules[0];
    assert!((bar.x - CM.null_delimiter_space).abs() < EPS, "{bar:?}");
    assert!((frac.width - (bar.width + 2.0 * CM.null_delimiter_space)).abs() < EPS);
    // An empty `\over` is the bare null fraction: TeX's 2.4 pt box at the
    // num1 and denom1 shifts.
    let over = e.typeset(r"\over", Style::Display).unwrap();
    assert!((over.width - 2.0 * CM.null_delimiter_space).abs() < EPS);
    assert!((over.height - CM.num1).abs() < EPS && (over.depth - CM.denom1).abs() < EPS);
}

#[test]
fn vdots_and_ddots_are_latex_period_constructions() {
    // LaTeX: \vdots stacks three text periods on 4 pt baselines under a 6 pt
    // kern; \ddots raises them 7, 4 and 1 pt between 1-2-2-1 mu kerns, the
    // highest under a 7 pt kern. Points and \hbox{.} keep their size in
    // scripts.
    let e = engine();
    let period = e.typeset(r"\text{.}", Style::Display).unwrap();
    let dots = |layout: &Layout| -> Vec<(f64, f64, f64)> {
        layout
            .glyphs
            .iter()
            .map(|g| {
                assert_eq!(g.ch, '.', "{layout:?}");
                (g.x, g.y, g.size)
            })
            .collect()
    };
    let vdots = e.typeset(r"\vdots", Style::Display).unwrap();
    let v = dots(&vdots);
    assert_eq!(v.len(), 3);
    // Top to bottom, as the vbox emits them.
    for (dot, y) in v.iter().zip([0.8, 0.4, 0.0]) {
        assert!(dot.0.abs() < EPS && (dot.1 - y).abs() < EPS && (dot.2 - 1.0).abs() < EPS);
    }
    assert!((vdots.width - period.width).abs() < EPS);
    assert!((vdots.height - (1.4 + period.height)).abs() < EPS);
    let scripted = e.typeset(r"x_{\vdots}", Style::Display).unwrap();
    assert!(
        scripted
            .glyphs
            .iter()
            .filter(|g| g.ch == '.')
            .all(|g| (g.size - 1.0).abs() < EPS),
        "{scripted:?}"
    );

    let ddots = e.typeset(r"\ddots", Style::Display).unwrap();
    let d = dots(&ddots);
    let mu = CM.quad / 18.0;
    assert_eq!(d.len(), 3);
    for (i, (dot, y)) in d.iter().zip([0.7, 0.4, 0.1]).enumerate() {
        let x = mu + f64::from(u8::try_from(i).unwrap()) * (period.width + 2.0 * mu);
        assert!((dot.0 - x).abs() < EPS && (dot.1 - y).abs() < EPS, "{d:?}");
    }
    assert!((ddots.width - (3.0 * period.width + 6.0 * mu)).abs() < EPS);
    assert!((ddots.height - (1.4 + period.height)).abs() < EPS);
}

#[test]
fn spacing_glue_matches_the_table() {
    let e = engine();
    // a+b: 4mu medium spaces around Bin in text/display.
    let ab = e.typeset("ab", Style::Display).unwrap();
    let apb = e.typeset("a+b", Style::Display).unwrap();
    let plus = e.typeset("+", Style::Display).unwrap();
    let kern_ab = {
        // width(a+b) = width(ab) + width(+) + 2×4mu ± the ab kern delta.
        let expected = ab.width + plus.width + 2.0 * 4.0 / 18.0;
        apb.width - expected
    };
    assert!(kern_ab.abs() < 0.02, "medium spacing off by {kern_ab}");
    // In script style the medium space vanishes: x^{a+b}.
    let sup = e.typeset("x^{a+b}", Style::Display).unwrap();
    let sup_ab = e.typeset("x^{ab}", Style::Display).unwrap();
    // The script's own `+` (cmr7's, the cut TeX sets scripts in).
    let plus_script_w = e.typeset("+", Style::Script).unwrap().width;
    assert!(
        (sup.width - (sup_ab.width + plus_script_w)).abs() < 0.02,
        "script-style spacing must be suppressed"
    );
}

#[test]
fn phantoms_occupy_the_right_dimensions() {
    let e = engine();
    let x = e.typeset("x", Style::Display).unwrap();
    let ph = e.typeset(r"\phantom{x}", Style::Display).unwrap();
    assert!(ph.glyphs.is_empty());
    assert!((ph.width - x.width).abs() < EPS);
    assert!((ph.height - x.height).abs() < EPS);
    let hp = e.typeset(r"\hphantom{x}", Style::Display).unwrap();
    assert!((hp.width - x.width).abs() < EPS && hp.height.abs() < EPS);
    let vp = e.typeset(r"\vphantom{x}", Style::Display).unwrap();
    assert!(vp.width.abs() < EPS && (vp.height - x.height).abs() < EPS);
}

#[test]
fn every_glyph_carries_its_span_and_face() {
    let src = r"\frac{a}{x} + \sqrt{y}";
    let l = engine().typeset(src, Style::Display).unwrap();
    assert!(fmd_math::paths::spans_cover(&l, src.len()));
    assert!(!l.glyphs.is_empty());
}

#[test]
fn formerly_pending_constructs_now_lay_out() {
    // The fm-kg9 frontier, crossed: environments and the stretchy
    // constructions produce real layouts (their dedicated fixtures live in
    // extensions.rs); the named-error contract still holds for what remains
    // outside the tier — precise, tier-tagged, never silent.
    let e = engine();
    let m = e
        .typeset(
            r"\begin{matrix} a & b \\ c & d \end{matrix}",
            Style::Display,
        )
        .unwrap();
    assert_eq!(m.glyphs.len(), 4);
    let b = e.typeset(r"\overbrace{x+y}", Style::Display).unwrap();
    assert!(b.paths.is_empty(), "cmex's brace pieces, not a drawn band");
    assert_eq!(
        b.glyphs
            .iter()
            .filter(|g| g.face == fmd_math::faces::FACE_MATH_EXTENSION)
            .count(),
        4,
        "the four brace pieces"
    );

    // `center` graduated from the tier-2 vocabulary: inside mathematics it
    // is now the *precise* text-mode-only refusal rather than a pending
    // construct, and the ratchet no longer counts it.
    let err = e
        .typeset(r"\begin{center} x \end{center}", Style::Display)
        .unwrap_err();
    assert_eq!(err.unsupported_construct(), None);
    assert!(
        err.to_string()
            .contains("text-mode line-alignment environment"),
        "{err}"
    );

    // The fm-j5t symbol tranche: every graduated symbol resolves to a real
    // bundled-face glyph in one string — no silent drops, no substitutions.
    let syms = e
        .typeset(
            r"a \nmid b \circlearrowleft \circlearrowright \i \j",
            Style::Display,
        )
        .unwrap();
    assert_eq!(syms.glyphs.len(), 7, "a, nmid, b, two arrows, dotless i, j");
    let op = e.typeset(r"\oiint_S f", Style::Display).unwrap();
    assert!(
        op.glyphs.iter().any(|g| g.ch == '∯'),
        "the surface integral operator renders"
    );

    // The dot accents (fm-j5t, fm-ew7o): amsmath's \dddot and \ddddot set the
    // base as an operator under a row of three or four text periods
    // (\limits, rule 13a). TeX's \showbox of `\dddot{x}` in display style:
    // x centred 2.14132 pt in; the periods' baselines 1.99998 pt over x's
    // top, less the vbox's -0.45529 pt of \vss (5.85023 pt up), from
    // amsmath's 1.66661 pt `\,` on at ecrm1000's 2.7771 pt advance.
    let dots3 = e.typeset(r"\dddot x", Style::Display).unwrap();
    assert_eq!(dots3.glyphs.len(), 4, "x and three periods");
    let dots4 = e.typeset(r"\ddddot x", Style::Display).unwrap();
    assert_eq!(dots4.glyphs.len(), 5, "x and four periods");
    let x = dots3.glyphs.iter().find(|g| g.ch == 'x').unwrap();
    assert!((x.x - 0.214132).abs() < 1e-5 && x.y.abs() < EPS, "{x:?}");
    let periods: Vec<_> = dots3.glyphs.iter().filter(|g| g.ch == '.').collect();
    assert_eq!(periods.len(), 3, "{dots3:?}");
    for (i, g) in periods.iter().enumerate() {
        let want_x = 0.166661 + 0.27771 * i as f64;
        assert!(
            (g.x - want_x).abs() < 1e-5
                && (g.y - 0.585023).abs() < 1e-5
                && (g.size - 1.0).abs() < EPS,
            "period {i}: {g:?}"
        );
    }
    // In script style the base and the limit spacings shrink (cmex7's ξ11
    // is 1.39998 pt), but the periods are text at the declared size.
    let script = e.typeset(r"\dddot x", Style::Script).unwrap();
    for g in script.glyphs.iter().filter(|g| g.ch == '.') {
        assert!(
            (g.y - 0.395858).abs() < 1e-5 && (g.size - 1.0).abs() < EPS,
            "{g:?}"
        );
    }

    // The labeled extensible arrows (fm-j5t): a drawn band stretched to
    // its script-style label, label riding above as a limit; \xmapsto
    // adds the origin bar (a second closed contour).
    let arrow = e.typeset(r"\xrightarrow{f} x", Style::Display).unwrap();
    assert_eq!(arrow.paths.len(), 1, "one drawn arrow band");
    let f = arrow.glyphs.iter().find(|g| g.ch == 'f').unwrap();
    assert!((f.size - 0.7).abs() < 1e-9, "script-size label");
    assert!(f.y > 0.2, "label rides above the axis band");
    let mapsto = e.typeset(r"\xmapsto{f}", Style::Display).unwrap();
    assert_eq!(
        mapsto.paths[0].contours.len(),
        2,
        "arrow plus the origin bar"
    );
    let below = e.typeset(r"\xrightarrow[g]{f} x", Style::Display).unwrap();
    let g = below.glyphs.iter().find(|gl| gl.ch == 'g').unwrap();
    assert!(g.y < 0.0, "below-label rides under the band");

    // \doublespacing (fm-j5t): the declaration stretches subsequent
    // stacked baselines by setspace's 1.667.
    let single = e.typeset_text("a \\\\ b").unwrap();
    let double = e.typeset_text("\\doublespacing a \\\\ b").unwrap();
    let gap = |l: &fmd_math::Layout| {
        let a = l.glyphs.iter().find(|g| g.ch == 'a').unwrap().y;
        let b = l.glyphs.iter().find(|g| g.ch == 'b').unwrap().y;
        a - b
    };
    assert!(
        (gap(&double) / gap(&single) - 1.667).abs() < 1e-6,
        "stretched skip: {} vs {}",
        gap(&double),
        gap(&single)
    );

    // A construct still outside the tier keeps the named-error contract:
    // precise, tier-tagged, never silent.
    let err = e.typeset(r"\dx", Style::Display).unwrap_err();
    assert_eq!(err.unsupported_construct(), Some(r"\dx"));
}

#[test]
fn a_line_break_unskips_the_space_before_it() {
    // LaTeX's `\\` begins with `\unskip`, so a space before the break does
    // not widen the line it ends: every glyph of a centered block sits
    // exactly where it would without that space. (A space after `\\` is
    // already swallowed, as LaTeX's star/option look-ahead swallows it.)
    let e = engine();
    let positions = |source: &str| -> Vec<(char, f64, f64)> {
        e.typeset_text(source)
            .unwrap()
            .glyphs
            .iter()
            .map(|g| (g.ch, g.x, g.y))
            .collect()
    };
    let bare = positions(r"\begin{center}ab\\cd\end{center}");
    for spaced in [
        r"\begin{center}ab \\cd\end{center}",
        r"\begin{center}ab \\ cd\end{center}",
        r"\begin{flushright}ab \\cd\end{flushright}",
    ] {
        let expected = if spaced.contains("flushright") {
            positions(r"\begin{flushright}ab\\cd\end{flushright}")
        } else {
            bare.clone()
        };
        let got = positions(spaced);
        assert_eq!(got.len(), expected.len(), "{spaced}");
        for (g, want) in got.iter().zip(&expected) {
            assert_eq!(g.0, want.0, "{spaced}");
            assert!(
                (g.1 - want.1).abs() < EPS && (g.2 - want.2).abs() < EPS,
                "{spaced}: {g:?} vs {want:?}"
            );
        }
    }
    // Prose line breaks outside an environment unskip too.
    let prose = e.typeset_text(r"ab \\cd").unwrap();
    let plain = e.typeset_text(r"ab\\cd").unwrap();
    let xs = |l: &Layout| l.glyphs.iter().map(|g| g.x).collect::<Vec<_>>();
    assert_eq!(xs(&prose), xs(&plain));
}

/// Rule 17 (tex.web §755): a math character with no subscript is followed
/// by a kern of its italic correction, so `f(` clears the f's overhang. A
/// subscripted character keeps δ out of its width, and a character inside a
/// word of a text face (TeX's math_text_char) gets none.
#[test]
fn italic_correction_kerns_follow_lone_math_characters() {
    let e = engine();
    // TeX's own boxes: each placed glyph's TFM character.
    let tfm = |l: &Layout, i: usize| {
        let g = &l.glyphs[i];
        e.faces().tfm(g.face, g.gid).expect("a TeX font's glyph")
    };
    let metrics = |l: &Layout, i: usize| {
        let (font, code) = tfm(l, i);
        font.char(code).expect("its TFM character")
    };

    let italic = e.typeset("f(", Style::Text).unwrap();
    let m = metrics(&italic, 0);
    assert!(
        (m.italic - 0.107_64).abs() < 1e-5,
        "cmmi10's f has its TFM italic correction: {m:?}"
    );
    let gap = italic.glyphs[1].x - italic.glyphs[0].x;
    assert!(
        (gap - (m.width + m.italic)).abs() < EPS,
        "f( advances by the f's width plus its italic correction: {gap} vs {m:?}"
    );

    let subscripted = e.typeset("f_i", Style::Text).unwrap();
    let gap = subscripted.glyphs[1].x - subscripted.glyphs[0].x;
    assert!(
        (gap - metrics(&subscripted, 0).width).abs() < EPS,
        "a subscript attaches at the nucleus width, without δ: {gap}"
    );

    // A group holding one Ord character is that character (tex.web §1186):
    // `{f}(` kerns like `f(`, and `{x}^2` places its script like `x^2`.
    let grouped = e.typeset("{f}(", Style::Text).unwrap();
    let gap = grouped.glyphs[1].x - grouped.glyphs[0].x;
    assert!(
        (gap - (m.width + m.italic)).abs() < EPS,
        "{{f}}( is f(: {gap} vs {m:?}"
    );
    let xs = |l: &Layout| l.glyphs.iter().map(|g| (g.x, g.y)).collect::<Vec<_>>();
    assert_eq!(
        xs(&e.typeset("{x}^2", Style::Text).unwrap()),
        xs(&e.typeset("x^2", Style::Text).unwrap())
    );
    // ...and an accent over `{v}` takes the math-character skew of `\vec v`.
    assert_eq!(
        xs(&e.typeset(r"\vec{v}", Style::Text).unwrap()),
        xs(&e.typeset(r"\vec v", Style::Text).unwrap())
    );

    // `\mathrm{f}(` is an upright f before an upright `(` of the same family:
    // TeX's math_text_char in a font with interword space, so δ = 0.
    let word = e.typeset(r"\mathrm{f}(", Style::Text).unwrap();
    let upright = metrics(&word, 0);
    assert!(
        upright.italic > 0.0,
        "the upright f overhangs too: {upright:?}"
    );
    let gap = word.glyphs[1].x - word.glyphs[0].x;
    assert!(
        (gap - upright.width).abs() < EPS,
        "inside a word of cmr10 the italic correction is zero: {gap} vs {upright:?}"
    );
    // Within `\mathrm{ff}` the first f is such a word character, and cmr10's
    // program pairs f with f only as a ligature, which the Reference's
    // preamble disables: no kern, no δ.
    let ff = e.typeset(r"\mathrm{ff}", Style::Text).unwrap();
    let ((font, first), (_, second)) = (tfm(&ff, 0), tfm(&ff, 1));
    assert_eq!(font.kern(first, second), None);
    let gap = ff.glyphs[1].x - ff.glyphs[0].x;
    assert!(
        (gap - upright.width).abs() < EPS,
        "\\mathrm{{ff}}: {gap} vs {upright:?}"
    );

    // A `\left…\right` inner is a sub-mlist, not a clean box: its lone f
    // keeps δ before the closing delimiter.
    let fenced = e.typeset(r"\left( f \right)", Style::Text).unwrap();
    let gap = fenced.glyphs[2].x - fenced.glyphs[1].x;
    assert!(
        (gap - (m.width + m.italic)).abs() < EPS,
        "\\left( f \\right): {gap} vs {m:?}"
    );
}

/// TeX's TFM boxes, not the ink: a character's box is its TFM height,
/// depth and width (floored at zero), cmmi's lig/kern program kerns an Ord
/// character against the next character of its font whatever that atom's
/// class, cmmi keeps δ between letters (its interword space is zero), and
/// an accent's skew is the base's kern with the skewchar. Each value below
/// is TeX's own box for the string under the Reference preamble (real
/// TeX's `\wd`, `\ht` and `\dp`, in ems).
#[test]
fn character_boxes_and_kerns_are_tex_metrics() {
    let e = engine();
    for (source, (w, h, d)) in [
        ("-", (0.777_78, 0.583_33, 0.083_33)),
        (r"\cdot", (0.277_78, 0.444_44, 0.0)),
        (r"\rightarrow", (1.0, 0.366_88, 0.0)),
        ("y", (0.526_16, 0.430_55, 0.194_44)),
        ("4", (0.5, 0.644_44, 0.0)),
        ("df", (0.951_05, 0.694_44, 0.194_44)),
        (r"\theta", (0.497_22, 0.694_44, 0.0)),
        // Clean boxes keep a lone character's italic correction in their
        // width (tex.web §721), and an accent's box is its width plus its
        // italic correction (§709): `\showbox` in display style.
        (r"\frac{f}{2}", (0.837_225, 1.370_952, 0.685_951)),
        ("x^f", (1.089_935, 0.899_002, 0.0)),
        (r"\sqrt{f}", (1.597_227, 0.983_821, 0.256_187)),
        (r"\tilde{f}", (0.597_226, 0.931_749, 0.194_444)),
        (r"\vec{v}", (0.520_601, 0.714_444, 0.0)),
    ] {
        let l = e.typeset(source, Style::Display).unwrap();
        assert!(
            (l.width - w).abs() < 1e-4 && (l.height - h).abs() < 1e-4 && (l.depth - d).abs() < 1e-4,
            "{source}: ({}, {}, {}) vs TeX ({w}, {h}, {d})",
            l.width,
            l.height,
            l.depth
        );
    }
    // `V,`: cmmi10 kerns V against its comma by -1/6 em.
    let gap = |source: &str| {
        let l = e.typeset(source, Style::Text).unwrap();
        l.glyphs[1].x - l.glyphs[0].x
    };
    let v = e.typeset("V", Style::Text).unwrap();
    let (font, code) = e.faces().tfm(v.glyphs[0].face, v.glyphs[0].gid).unwrap();
    let ch = font.char(code).unwrap();
    assert!((gap("V,") - (ch.width + ch.italic - 0.166_67)).abs() < 1e-4);
    // Two letters keep the first one's δ.
    let (font, code) = {
        let y = e.typeset("y", Style::Text).unwrap();
        e.faces().tfm(y.glyphs[0].face, y.glyphs[0].gid).unwrap()
    };
    let y = font.char(code).unwrap();
    assert!((gap("yx") - (y.width + y.italic)).abs() < EPS);
    // `\hat{A}` skews by cmmi10's A–skewchar kern (5/36 em) over the
    // accent's centered position.
    let skewed = e.typeset(r"\hat{A}", Style::Text).unwrap();
    let centered = e.typeset(r"\hat{\mathrm{A}}", Style::Text).unwrap();
    let accent_x = |l: &Layout| l.glyphs.iter().find(|g| g.ch != 'A').unwrap().x;
    let a_width = |l: &Layout| l.width;
    let shift = (accent_x(&skewed) - a_width(&skewed) / 2.0)
        - (accent_x(&centered) - a_width(&centered) / 2.0);
    assert!((shift - 0.138_89).abs() < 1e-3, "skew {shift}");
}

#[test]
fn ellipses_are_three_thin_spaced_punctuation_dots() {
    // Plain TeX: `\cdots` is `\mathinner{\cdotp\cdotp\cdotp}` and `\ldots`
    // the same with `\ldotp`. Punct-punct glue is a thin space (3 mu) in
    // display and text styles and nothing in script styles.
    let e = engine();
    for (command, dot, single) in [
        (r"\cdots", '⋅', r"\cdot"),
        (r"\ldots", '.', "."),
        (r"\hdots", '.', "."),
    ] {
        let xs = |layout: &Layout| -> Vec<f64> {
            layout
                .glyphs
                .iter()
                .filter(|glyph| glyph.ch == dot)
                .map(|glyph| glyph.x)
                .collect()
        };
        let display = e.typeset(command, Style::Display).unwrap();
        let script = e
            .typeset(&format!("x_{{{command}}}"), Style::Display)
            .unwrap();
        let (d, s) = (xs(&display), xs(&script));
        assert_eq!(display.glyphs.len(), 3, "{command}: {display:?}");
        assert_eq!(s.len(), 3, "{command}: {script:?}");
        // Each size's own dot: scripts set the 7-point cut, so the script
        // advance is measured at script style, not scaled from 10 points.
        let advance = e.typeset(single, Style::Display).unwrap().width;
        let script_advance = e.typeset(single, Style::Script).unwrap().width;
        for pair in s.windows(2) {
            assert!(
                (pair[1] - pair[0] - script_advance).abs() < EPS,
                "{command}: script dots {s:?} must abut (advance {script_advance})"
            );
        }
        for pair in d.windows(2) {
            let gap = pair[1] - pair[0];
            assert!(
                (gap - (advance + 3.0 * CM.quad / 18.0)).abs() < EPS,
                "{command}: gap {gap} vs advance {advance} + 3 mu"
            );
        }
    }
}

#[test]
fn tbinom_and_dbinom_force_their_styles() {
    // amsmath: \tbinom is \genfrac(){0pt}{1} (text style), \dbinom is
    // \genfrac(){0pt}{0} (display style), whatever the surroundings.
    let e = engine();
    let height = |source: &str, style: Style| {
        let layout = e
            .typeset(source, style)
            .unwrap_or_else(|err| panic!("`{source}`: {err}"));
        layout.height + layout.depth
    };
    let display = height(r"\binom{N}{4}", Style::Display);
    let text = height(r"\binom{N}{4}", Style::Text);
    assert!(text < display, "text {text} vs display {display}");
    assert!((height(r"\tbinom{N}{4}", Style::Display) - text).abs() < EPS);
    assert!((height(r"\dbinom{N}{4}", Style::Text) - display).abs() < EPS);
    for (forced, plain, target) in [
        (r"\dbinom{N}{4}", r"\binom{N}{4}", Style::Display),
        (r"\tbinom{N}{4}", r"\binom{N}{4}", Style::Text),
        (r"\dfrac{N}{4}", r"\frac{N}{4}", Style::Display),
        (r"\tfrac{N}{4}", r"\frac{N}{4}", Style::Text),
    ] {
        let expected = e.typeset(plain, target).unwrap();
        for surrounding in [
            Style::Display,
            Style::Text,
            Style::Script,
            Style::ScriptScript,
        ] {
            let actual = e.typeset(forced, surrounding).unwrap();
            for (dimension, value, want) in [
                ("width", actual.width, expected.width),
                ("height", actual.height, expected.height),
                ("depth", actual.depth, expected.depth),
            ] {
                assert!(
                    (value - want).abs() < EPS,
                    "{forced} in {surrounding:?}: {dimension} {value} vs {want}"
                );
            }
        }
    }
}

#[test]
fn forced_fraction_script_boxes_match_tex() {
    // The grammar differential's three reported boxes plus fresh isolated
    // LaTeX/T1/amsmath showbox observations, including subscripts/radicands
    // with a superscript in the numerator (uncramped forced styles).
    // All values are ems at the Reference's 10 pt text size.
    let e = engine();
    let mut mismatches = Vec::new();
    for (source, expected) in [
        (
            r"{\scriptstyle \dbinom{B}{A \sim 6 = \Omega}}",
            [6.1111, 1.45001, 0.950012],
        ),
        (r"{1^{3}}^{\dbinom{m}{b}}", [3.348857, 2.507661, 0.0]),
        (r"C_{\dbinom{7}{m = P}}", [5.22919, 0.683331, 2.05558]),
        (
            r"{\scriptstyle \dbinom{B}{A \sim 6 = Z}}",
            [6.143044, 1.45001, 0.950012],
        ),
        (
            r"{\scriptscriptstyle \dbinom{B}{A \sim 6 = Z}}",
            [6.143044, 1.45001, 0.950012],
        ),
        (r"x_{\dbinom{a^2}{b}}", [3.070958, 0.430554, 2.09608]),
        (r"\sqrt{\dbinom{a^2}{b}}", [3.449432, 1.904079, 1.135948]),
        (r"x_{\dfrac{a^2}{b}}", [1.838728, 0.430554, 1.832019]),
        (r"\sqrt{\dfrac{a^2}{b}}", [2.217203, 1.736107, 0.703914]),
        (r"x_{\tbinom{a^2}{b}}", [2.362247, 0.430554, 1.073294]),
        (r"\sqrt{\tbinom{a^2}{b}}", [2.740721, 1.392686, 0.447328]),
        (r"x_{\tfrac{a^2}{b}}", [1.685574, 0.430554, 1.01813]),
        (r"\sqrt{\tfrac{a^2}{b}}", [2.064049, 1.37027, 0.469745]),
        (
            r"x_{{\displaystyle\binom{a^2}{b}}}",
            [3.070958, 0.430554, 2.09608],
        ),
        (
            r"\sqrt{{\displaystyle\binom{a^2}{b}}}",
            [3.449432, 1.904079, 1.135948],
        ),
        (
            r"x_{{\displaystyle\frac{a^2}{b}}}",
            [1.838728, 0.430554, 1.832019],
        ),
        (
            r"\sqrt{{\displaystyle\frac{a^2}{b}}}",
            [2.217203, 1.736107, 0.703914],
        ),
        (
            r"x_{{\textstyle\binom{a^2}{b}}}",
            [2.362247, 0.430554, 1.073294],
        ),
        (
            r"\sqrt{{\textstyle\binom{a^2}{b}}}",
            [2.740721, 1.392686, 0.447328],
        ),
        (
            r"x_{{\textstyle\frac{a^2}{b}}}",
            [1.685574, 0.430554, 1.01813],
        ),
        (
            r"\sqrt{{\textstyle\frac{a^2}{b}}}",
            [2.064049, 1.37027, 0.469745],
        ),
    ] {
        let layout = e.typeset(source, Style::Display).unwrap();
        let actual = [layout.width, layout.height, layout.depth];
        if actual
            .iter()
            .zip(expected)
            .any(|(value, want)| (value - want).abs() >= 1e-4)
        {
            mismatches.push(format!("{source}: actual {actual:?}, TeX {expected:?}"));
        }
    }
    assert!(mismatches.is_empty(), "{}", mismatches.join("\n"));
}

#[test]
fn math_lines_open_up_by_jot_and_text_lines_do_not() {
    // The Tex surface's top-level `\\` rows are align* rows: amsmath's
    // \openup\jot puts their baselines \baselineskip + \jot = 15 pt apart at
    // 10 pt. A TexText paragraph keeps plain \baselineskip, 12 pt.
    let e = engine();
    let math = e.typeset(r"a \\ b", Style::Display).unwrap();
    let rows = glyph_y(&math, 'a') - glyph_y(&math, 'b');
    assert!(
        (rows - (CM.baseline_skip + CM.jot)).abs() < EPS,
        "math rows {rows}"
    );
    let text = e.typeset_text(r"a\\b").unwrap();
    let lines = glyph_y(&text, 'a') - glyph_y(&text, 'b');
    assert!((lines - CM.baseline_skip).abs() < EPS, "text lines {lines}");
}

#[test]
fn tex_rows_follow_align_star() {
    // The Reference sets Tex in align*: `&` cells pair right/left around the
    // alignment point, a right-hand cell opens with amsmath's `{}`, and a row
    // without `&` sits in the right-aligned first column. TexText lines stay
    // flush left.
    let e = engine();
    let x = |layout: &Layout, ch: char| -> f64 {
        layout
            .glyphs
            .iter()
            .find(|g| g.ch == ch)
            .unwrap_or_else(|| panic!("glyph {ch} in {layout:?}"))
            .x
    };
    // `1 &= 2` is as wide as `1 = 2`: the empty Ord keeps `=` a relation.
    let aligned = e.typeset(r"1 &= 2", Style::Display).unwrap();
    let plain = e.typeset(r"1 = 2", Style::Display).unwrap();
    assert!((aligned.width - plain.width).abs() < EPS, "{aligned:?}");
    assert!((x(&aligned, '=') - x(&plain, '=')).abs() < EPS);
    // Rows align at `&`.
    let rows = e.typeset(r"1 + 1 &= 2 \\ 3 &= 4", Style::Display).unwrap();
    let eqs: Vec<f64> = rows
        .glyphs
        .iter()
        .filter(|g| g.ch == '=')
        .map(|g| g.x)
        .collect();
    assert_eq!(eqs.len(), 2);
    assert!((eqs[0] - eqs[1]).abs() < EPS, "{eqs:?}");
    // A row without `&` is right-aligned: `1` ends where `222` does.
    let right = e.typeset(r"1 \\ 222", Style::Display).unwrap();
    let one = e.typeset("1", Style::Display).unwrap();
    assert!((x(&right, '1') + one.width - right.width).abs() < EPS);
    let text = e.typeset_text(r"1\\222").unwrap();
    assert!(x(&text, '1').abs() < EPS);
}

#[test]
fn script_boxes_carry_scriptspace() {
    // §756: each sub- and superscript box is widened by \scriptspace
    // (0.5 pt), in every style; limits are not scripts and get none.
    let e = engine();
    let two = e.typeset("2", Style::Script).unwrap().width;
    for src in ["{}^2", "{}_2"] {
        let w = e.typeset(src, Style::Display).unwrap().width;
        assert!(
            (w - (two + CM.script_space)).abs() < EPS,
            "{src}: {w} vs {two}"
        );
    }
    let both = e.typeset("{}^2_2", Style::Display).unwrap().width;
    assert!((both - (two + CM.script_space)).abs() < EPS, "{both}");
    let limits = e.typeset(r"\sum_2", Style::Display).unwrap().width;
    let sum = e.typeset(r"\sum", Style::Display).unwrap().width;
    assert!((limits - sum).abs() < EPS, "limits {limits} vs {sum}");
}

#[test]
fn array_rows_carry_latex_struts() {
    // LaTeX's array (amsmath's matrices, cases): every row carries
    // \@arstrut, 0.7 and 0.3 \baselineskip times \arraystretch, and rows
    // stack depth to height. Measured in LaTeX: a two-row matrix of
    // letters is 1.45 em high and 0.95 em deep, a three-row one 3.6 em in
    // all; cases rows are 1.44 em apart (\arraystretch 1.2).
    let e = engine();
    let two = e
        .typeset(r"\begin{matrix} a \\ b \end{matrix}", Style::Display)
        .unwrap();
    assert!(
        (two.height - 1.45).abs() < EPS && (two.depth - 0.95).abs() < EPS,
        "{two:?}"
    );
    let three = e
        .typeset(r"\begin{matrix} a \\ b \\ c \end{matrix}", Style::Display)
        .unwrap();
    assert!((three.height + three.depth - 3.6).abs() < EPS, "{three:?}");
    let cases = e
        .typeset(r"\begin{cases} a & b \\ c & d \end{cases}", Style::Display)
        .unwrap();
    let pitch = glyph_y(&cases, 'a') - glyph_y(&cases, 'c');
    assert!((pitch - 1.44).abs() < EPS, "cases rows {pitch}");
}

#[test]
fn accents_take_tex_accent_geometry() {
    // Rule 12 with plain TeX's accent characters (cmr10's, cmmi10's vector):
    // the accent ink sits where TeX's does, whichever face supplies it.
    let e = engine();
    let one = e.typeset("1", Style::Display).unwrap();
    let accent = |layout: &Layout, base: char| {
        layout
            .glyphs
            .iter()
            .find(|g| g.ch != base)
            .cloned()
            .unwrap_or_else(|| panic!("an accent glyph in {layout:?}"))
    };
    // `\vec` is cmmi10's own vector, its 0.44 em arrow above the letter,
    // topping out at 0.714 em, not a full-size arrow through it.
    let vec = e.typeset(r"\vec{v}", Style::Display).unwrap();
    let arrow = accent(&vec, 'v');
    assert_eq!(
        (arrow.face, arrow.size),
        (fmd_math::faces::FACE_MATH_ITALIC, 1.0),
        "{arrow:?}"
    );
    assert!((vec.height - 0.714).abs() < 0.02, "{vec:?}");
    // `\hat` is CM's own circumflex (cmr10's), topping out 0.694 em above
    // its box, which rises by h - min(h, x-height): barely over an x-height
    // letter, by 0.25 em over `A`. (CM Unicode's circumflex is 0.013 em
    // taller than cmr10's.)
    let hat = e.typeset(r"\hat{x}", Style::Display).unwrap();
    assert_eq!(accent(&hat, 'x').face, one.glyphs[0].face);
    for (base, accented) in [("x", r"\hat{x}"), ("A", r"\hat{A}")] {
        let h = e.typeset(base, Style::Display).unwrap().height;
        let laid = e.typeset(accented, Style::Display).unwrap().height;
        let tex = h - h.min(CM.x_height) + 0.694;
        assert!((laid - tex).abs() < 0.02, "{accented}: {laid} vs {tex}");
    }
}

#[test]
fn character_accents_attach_scripts_before_packing_the_accent() {
    // Independent 10 pt TeX boxes with the ruled Reference preamble
    // (fm-sup-accented-nucleus-r12-a529). Rule 12 transfers the scripts
    // to the character; the accent's full height must not raise them.
    let e = engine();
    for (source, expected) in [
        (r"{\bar{\phi}}^{y}", [1.076_509, 0.831_667, 0.194_444]),
        (r"{\hat{x}}^{2}", [1.020_140, 0.864_003, 0.0]),
        (r"{\bar{A}}_{1}^{2}", [1.198_615, 0.864_003, 0.247_217]),
        (r"\bar{\phi}^{y}", [1.076_509, 0.831_667, 0.194_444]),
        (r"\hat{x}^{2}", [1.020_140, 0.864_003, 0.0]),
        (r"\bar{A}_{1}^{2}", [1.198_615, 0.864_003, 0.247_217]),
        (r"\phi^{y}", [1.076_509, 0.714_280, 0.194_444]),
        (r"{\phi}^{y}", [1.076_509, 0.714_280, 0.194_444]),
        (r"{\bar{f}}_1", [0.938_199, 0.831_667, 0.194_444]),
        (r"{\bar{f}}^2", [1.045_839, 0.864_003, 0.194_444]),
        (r"{\bar{f}}_1^2", [1.045_839, 0.864_003, 0.247_217]),
        (r"{\hat{f}}_1", [0.938_199, 0.958_334, 0.194_444]),
        (r"{\hat{xy}}^2", [1.546_300, 0.898_335, 0.194_444]),
        (r"{\bar{xy}}_1^2", [1.546_300, 0.864_003, 0.247_217]),
        (r"{\overline{x}}^2", [1.020_140, 0.864_003, 0.0]),
        (r"{{\hat{x}}}^2", [1.020_140, 0.864_003, 0.0]),
        (r"\scriptstyle{\hat{x}}^2", [0.843_756, 0.624_000, 0.0]),
        (
            r"\scriptscriptstyle{\hat{x}}^2",
            [0.786_119, 0.523_997, 0.0],
        ),
        (r"{\bar{\frac{a}{b}}}^2", [1.217_201, 1.448_175, 0.685_951]),
        (r"{\vec{A}}_1^2", [1.198_615, 0.967_221, 0.247_217]),
        (r"{\dot{\phi}}^y", [1.076_509, 0.931_749, 0.194_444]),
        (r"{\widehat{x}}^2", [1.020_140, 0.864_003, 0.0]),
        (r"{\widetilde{x}}^2", [1.020_140, 0.864_003, 0.0]),
        (r"x_{{\hat{x}}^2}", [1.465_283, 0.430_554, 0.177_777]),
        (r"x^{{\bar{A}}_1^2}", [1.613_202, 1.036_893, 0.0]),
    ] {
        let layout = e.typeset(source, Style::Display).unwrap();
        let actual = [layout.width, layout.height, layout.depth];
        for (got, want) in actual.into_iter().zip(expected) {
            assert!(
                (got - want).abs() < 1e-4,
                "{source}: {actual:?} vs {expected:?}"
            );
        }
    }
}

#[test]
fn wide_hats_match_independent_tex_boxes() {
    // Ruled Reference preamble at 10pt, independently captured before
    // evaluating this candidate (fm-widehat-height-r12-v3jj). The last
    // thirteen cases exercise amsfonts' TEXT-style, two-text-em branch,
    // including all three optical sizes and its strict boundary. The
    // supported \hspace boundary inputs were independently measured to
    // have the same TeX boxes as the original, unsupported \kern inputs;
    // those original inputs retain their precise-error checks below.
    let e = engine();
    for (source, expected) in [
        (r"\widehat{P}", [0.780_904, 0.975_000, 0.0]),
        (r"\widehat{8}", [0.500_002, 0.936_113, 0.0]),
        (r"\widehat{x}", [0.571_527, 0.722_223, 0.0]),
        (r"\widehat{y}", [0.526_160, 0.722_223, 0.194_444]),
        (r"\widehat{f}", [0.597_226, 0.986_113, 0.194_444]),
        (r"\widehat{M}", [1.079_166, 1.002_777, 0.0]),
        (r"\widehat{xy}", [1.097_687, 0.750_000, 0.194_444]),
        (r"\widehat{AB}", [1.558_682, 1.002_777, 0.0]),
        (r"\widehat{xyz}", [1.606_717, 0.750_000, 0.194_444]),
        (r"\widehat{ABC}", [2.344_931, 1.045_372, 0.0]),
        (r"\widehat{wxyz}", [2.349_542, 0.825_005, 0.194_444]),
        (r"\widehat{ABCD}", [3.200_626, 1.045_372, 0.0]),
        (r"\widehat{}", [0.0, 0.722_223, 0.0]),
        (r"\widehat{\frac{a}{b}}", [0.768_588, 1.398_730, 0.685_951]),
        (r"\widehat{\widehat{x}}", [0.571_527, 1.013_892, 0.0]),
        (r"\scriptstyle\widehat{P}", [0.616_771, 0.682_500, 0.0]),
        (r"\scriptstyle\widehat{8}", [0.398_613, 0.655_276, 0.0]),
        (
            r"\scriptstyle\widehat{xy}",
            [0.884_148, 0.525_000, 0.136_110],
        ),
        (
            r"\scriptscriptstyle\widehat{P}",
            [0.524_484, 0.487_500, 0.0],
        ),
        (
            r"\scriptscriptstyle\widehat{8}",
            [0.340_283, 0.468_054, 0.0],
        ),
        (
            r"\scriptscriptstyle\widehat{xy}",
            [0.779_056, 0.375_000, 0.097_221],
        ),
        (r"\widehat{\text{P}}", [0.680_390, 0.980_390, 0.0]),
        (r"\widehat{{P}}", [0.780_904, 0.975_000, 0.0]),
        (r"\widehat{P}^2", [1.229_517, 0.975_000, 0.0]),
        (r"\widehat{y}_1", [0.938_895, 0.722_223, 0.194_444]),
        (r"\scriptstyle\widehat{ABC}", [1.862_290, 0.731_573, 0.0]),
        (r"\scriptstyle\widehat{ABCD}", [2.537_666, 0.731_573, 0.0]),
        (
            r"\scriptscriptstyle\widehat{ABC}",
            [1.591_930, 0.524_794, 0.0],
        ),
        (
            r"\scriptscriptstyle\widehat{ABCD}",
            [2.164_229, 0.524_794, 0.0],
        ),
        (r"\widehat{\hspace{1.9999em}}", [1.999_405, 0.750_000, 0.0]),
        (r"\widehat{\hspace{2em}}", [1.999_512, 0.750_000, 0.0]),
        (r"\widehat{\hspace{2.0001em}}", [1.999_617, 0.825_005, 0.0]),
        (
            r"\scriptstyle\widehat{\hspace{1.9999em}}",
            [1.999_405, 0.525_000, 0.0],
        ),
        (
            r"\scriptstyle\widehat{\hspace{2em}}",
            [1.999_512, 0.525_000, 0.0],
        ),
        (
            r"\scriptstyle\widehat{\hspace{2.0001em}}",
            [1.999_617, 0.577_310, 0.0],
        ),
        (
            r"\scriptscriptstyle\widehat{\hspace{1.9999em}}",
            [1.999_405, 0.375_000, 0.0],
        ),
        (
            r"\scriptscriptstyle\widehat{\hspace{2em}}",
            [1.999_512, 0.375_000, 0.0],
        ),
        (
            r"\scriptscriptstyle\widehat{\hspace{2.0001em}}",
            [1.999_617, 0.414_603, 0.0],
        ),
    ] {
        let layout = e.typeset(source, Style::Display).unwrap();
        let actual = [layout.width, layout.height, layout.depth];
        for (got, want) in actual.into_iter().zip(expected) {
            assert!(
                (got - want).abs() < 1e-4,
                "{source}: {actual:?} vs independent TeX {expected:?}"
            );
        }
    }
    for source in [
        r"\widehat{\kern1.9999em}",
        r"\widehat{\kern2em}",
        r"\widehat{\kern2.0001em}",
        r"\scriptstyle\widehat{\kern1.9999em}",
        r"\scriptstyle\widehat{\kern2em}",
        r"\scriptstyle\widehat{\kern2.0001em}",
        r"\scriptscriptstyle\widehat{\kern1.9999em}",
        r"\scriptscriptstyle\widehat{\kern2em}",
        r"\scriptscriptstyle\widehat{\kern2.0001em}",
    ] {
        assert!(
            matches!(
                e.typeset(source, Style::Display),
                Err(fmd_math::MathError::UnsupportedCommand { name, .. }) if name == r"\kern"
            ),
            "{source}: unsupported primitive must retain its precise error"
        );
    }
}

#[test]
fn wide_tildes_match_independent_tex_boxes() {
    // fm-ifgjb: the 10pt ruled-Reference table was captured before editing
    // production. Cover the CMEX/MSBM successor chains, all three optical
    // sizes, and amsfonts' strict two-TEXT-em boundary. Independent TeX
    // captures prove the supported hspace forms have exactly the same
    // boxes as the original kern forms, retained as named errors below.
    let e = engine();
    for (source, expected) in [
        (r"\widetilde{P}", [0.780_904, 0.975_000, 0.0]),
        (r"\widetilde{8}", [0.500_002, 0.936_113, 0.0]),
        (r"\widetilde{x}", [0.571_527, 0.722_223, 0.0]),
        (r"\widetilde{y}", [0.526_160, 0.722_223, 0.194_444]),
        (r"\widetilde{f}", [0.597_226, 0.986_113, 0.194_444]),
        (r"\widetilde{M}", [1.079_166, 1.002_777, 0.0]),
        (r"\widetilde{xy}", [1.097_687, 0.750_000, 0.194_444]),
        (r"\widetilde{AB}", [1.558_682, 1.002_777, 0.0]),
        (r"\widetilde{xyz}", [1.606_717, 0.750_000, 0.194_444]),
        (r"\widetilde{ABC}", [2.344_931, 1.120_366, 0.0]),
        (r"\widetilde{wxyz}", [2.349_542, 0.899_998, 0.194_444]),
        (r"\widetilde{ABCD}", [3.200_626, 1.120_366, 0.0]),
        (r"\widetilde{}", [0.0, 0.722_223, 0.0]),
        (
            r"\widetilde{\frac{a}{b}}",
            [0.768_588, 1.398_730, 0.685_951],
        ),
        (r"\widetilde{\widetilde{x}}", [0.571_527, 1.013_892, 0.0]),
        (r"\widetilde{\text{P}}", [0.680_390, 0.980_390, 0.0]),
        (r"\widetilde{{P}}", [0.780_904, 0.975_000, 0.0]),
        (r"\widetilde{P}^2", [1.229_517, 0.975_000, 0.0]),
        (r"\widetilde{y}_1", [0.938_895, 0.722_223, 0.194_444]),
        (r"\scriptstyle\widetilde{P}", [0.616_771, 0.682_500, 0.0]),
        (
            r"\scriptstyle\widetilde{xy}",
            [0.884_148, 0.525_000, 0.136_110],
        ),
        (r"\scriptstyle\widetilde{ABC}", [1.862_290, 0.784_262, 0.0]),
        (r"\scriptstyle\widetilde{ABCD}", [2.537_666, 0.784_262, 0.0]),
        (
            r"\scriptscriptstyle\widetilde{P}",
            [0.524_484, 0.487_500, 0.0],
        ),
        (
            r"\scriptscriptstyle\widetilde{xy}",
            [0.779_056, 0.375_000, 0.097_221],
        ),
        (
            r"\scriptscriptstyle\widetilde{ABC}",
            [1.591_930, 0.561_388, 0.0],
        ),
        (
            r"\scriptscriptstyle\widetilde{ABCD}",
            [2.164_229, 0.561_388, 0.0],
        ),
        (
            r"\widetilde{\hspace{1.9999em}}",
            [1.999_405, 0.750_000, 0.0],
        ),
        (r"\widetilde{\hspace{2em}}", [1.999_512, 0.750_000, 0.0]),
        (
            r"\widetilde{\hspace{2.0001em}}",
            [1.999_617, 0.825_005, 0.0],
        ),
        (
            r"\scriptstyle\widetilde{\hspace{1.9999em}}",
            [1.999_405, 0.525_000, 0.0],
        ),
        (
            r"\scriptstyle\widetilde{\hspace{2em}}",
            [1.999_512, 0.525_000, 0.0],
        ),
        (
            r"\scriptstyle\widetilde{\hspace{2.0001em}}",
            [1.999_617, 0.629_999, 0.0],
        ),
        (
            r"\scriptscriptstyle\widetilde{\hspace{1.9999em}}",
            [1.999_405, 0.375_000, 0.0],
        ),
        (
            r"\scriptscriptstyle\widetilde{\hspace{2em}}",
            [1.999_512, 0.375_000, 0.0],
        ),
        (
            r"\scriptscriptstyle\widetilde{\hspace{2.0001em}}",
            [1.999_617, 0.451_196, 0.0],
        ),
    ] {
        let layout = e.typeset(source, Style::Display).unwrap();
        let actual = [layout.width, layout.height, layout.depth];
        for (got, want) in actual.into_iter().zip(expected) {
            assert!(
                got.is_finite() && (got - want).abs() < 1e-4,
                "{source}: {actual:?} vs independent TeX {expected:?}"
            );
        }
    }
    for source in [
        r"\kern2em",
        r"\widetilde{\kern1.9999em}",
        r"\widetilde{\kern2em}",
        r"\widetilde{\kern2.0001em}",
        r"\scriptstyle\widetilde{\kern1.9999em}",
        r"\scriptstyle\widetilde{\kern2em}",
        r"\scriptstyle\widetilde{\kern2.0001em}",
        r"\scriptscriptstyle\widetilde{\kern1.9999em}",
        r"\scriptscriptstyle\widetilde{\kern2em}",
        r"\scriptscriptstyle\widetilde{\kern2.0001em}",
    ] {
        assert!(
            matches!(
                e.typeset(source, Style::Display),
                Err(fmd_math::MathError::UnsupportedCommand { name, .. }) if name == r"\kern"
            ),
            "{source}: unsupported primitive must retain its precise error"
        );
    }
}

#[test]
fn wide_accent_measurements_start_fresh_math_alphabets() {
    // amsfonts measures the nucleus in a fresh text-style formula before
    // selecting CMEX/MSBM. An outer alphabet changes the actual nucleus,
    // but must not change that measurement. An explicit inner alphabet does.
    // Independent ruled-Reference boxes, captured before repairing this path.
    let e = engine();
    for (source, expected) in [
        (
            r"\mathrm{\widetilde{AP\hspace{0.48em}}}",
            [1.910_437, 1.045_372, 0.0],
        ),
        (
            r"\widetilde{\mathrm{AP\hspace{0.48em}}}",
            [1.910_437, 1.002_777, 0.0],
        ),
        (
            r"\mathrm{\widehat{AP\hspace{0.48em}}}",
            [1.910_437, 1.045_372, 0.0],
        ),
        (
            r"\widehat{\mathrm{AP\hspace{0.48em}}}",
            [1.910_437, 1.002_777, 0.0],
        ),
        (
            r"\widetilde{AP\hspace{0.48em}}",
            [2.010_783, 1.045_372, 0.0],
        ),
        (r"\widehat{AP\hspace{0.48em}}", [2.010_783, 1.045_372, 0.0]),
    ] {
        let layout = e.typeset(source, Style::Display).unwrap();
        let actual = [layout.width, layout.height, layout.depth];
        for (got, want) in actual.into_iter().zip(expected) {
            assert!(
                got.is_finite() && (got - want).abs() < 1e-4,
                "{source}: {actual:?} vs independent TeX {expected:?}"
            );
        }
    }
}

#[test]
fn nested_wide_hats_reuse_text_style_measurements() {
    let e = engine();
    let mut source = String::from("x");
    for _ in 0..24 {
        source = format!("\\widehat{{{source}}}");
    }
    let layout = e.typeset(&source, Style::Display).unwrap();
    assert_eq!(layout.paths.len(), 24);
    assert_eq!(layout.glyphs.len(), 1);
    // Published CMEX10 TFM heights and x-height, not the implementation's
    // measured result. First hat replaces x's height; each later one
    // adds the accent height minus the font's x-height overlap.
    let expected_height =
        0.722_223_281_860_351_6 + 23.0 * (0.722_223_281_860_351_6 - 0.430_555_343_627_929_7);
    assert!((layout.height - expected_height).abs() < 1e-4);
}

#[test]
fn wide_hat_measurements_preserve_distinct_macro_nuclei() {
    let e = engine();
    let mut macros = fmd_math::MacroSet::new();
    macros
        .define("hats", 0, r"\widehat{P}+\widehat{ABCD}")
        .unwrap();
    // Expanded tokens share their macro call's source span. They must
    // still measure like the same expressions written out independently.
    for style in [
        Style::Display,
        Style::Text,
        Style::Script,
        Style::ScriptScript,
    ] {
        let from_macro = e
            .typeset_with_macros(r"\widehat{\hats}", style, &macros)
            .unwrap();
        let expanded = e
            .typeset(r"\widehat{\widehat{P}+\widehat{ABCD}}", style)
            .unwrap();
        assert_eq!(
            [from_macro.width, from_macro.height, from_macro.depth],
            [expanded.width, expanded.height, expanded.depth],
            "{style:?}"
        );
        assert_eq!(from_macro.glyphs.len(), expanded.glyphs.len());
        assert_eq!(from_macro.paths.len(), expanded.paths.len());
    }
}

#[test]
fn underlines_match_independent_tex_boxes() {
    // Fresh ruled-Reference TeX at10pt (fm-underline-depth-r12-gj19).
    // All31 predeclared cases stay here, including plain/overline controls,
    // text islands, deep nuclei, nested boxes and all four math styles.
    // TeX's make_under adds5theta below the nucleus, not6theta.
    let e = engine();
    for (source, expected) in [
        (r"\underline{B}", [0.808_680, 0.683_331, 0.199_990]),
        (r"\underline{y}", [0.526_160, 0.430_554, 0.394_434]),
        (r"\underline{\Delta}", [0.833_336, 0.683_331, 0.199_990]),
        (r"\underline{\phi}", [0.595_834, 0.694_444, 0.394_434]),
        (r"\underline{xy}", [1.097_687, 0.430_554, 0.394_434]),
        (
            r"\underline{\frac{B}{y}}",
            [1.048_680, 1.359_839, 1.080_385],
        ),
        (r"\underline{\sqrt{y}}", [1.359_496, 0.751_869, 0.488_116]),
        (
            r"\underline{\underline{B}}",
            [0.808_680, 0.683_331, 0.399_979],
        ),
        (
            r"\underline{\underline{y}}",
            [0.526_160, 0.430_554, 0.594_423],
        ),
        (
            r"\overline{\underline{B}}",
            [0.808_680, 0.883_320, 0.199_990],
        ),
        (
            r"\underline{\overline{B}}",
            [0.808_680, 0.883_320, 0.199_990],
        ),
        (r"\underline{B}_1^2", [1.257_294, 0.887_222, 0.249_988]),
        (r"\underline{y}_1^2", [0.974_773, 0.864_003, 0.444_432]),
        (
            r"\frac{\underline{B}}{\underline{y}}",
            [1.048_680, 1.359_839, 1.080_385],
        ),
        (r"\sqrt{\underline{y}}", [1.526_161, 0.751_881, 0.488_127]),
        (
            r"\sum_{\underline{y}}^{\underline{B}}",
            [1.444_447, 1.909_442, 1.556_107],
        ),
        (r"\text{\underline{B}}", [0.708_160, 0.688_720, 0.199_990]),
        (r"\text{\underline{y}}", [0.527_649, 0.430_450, 0.394_386]),
        (r"\underline{\text{By}}", [1.235_810, 0.688_720, 0.394_386]),
        ("B", [0.808_680, 0.683_331, 0.0]),
        ("y", [0.526_160, 0.430_554, 0.194_444]),
        (r"\overline{B}", [0.808_680, 0.883_320, 0.0]),
        (r"\overline{y}", [0.526_160, 0.630_544, 0.194_444]),
        // The existing layout golden was separately measured before its
        // depth-only expectation was adjudicated; its ink is unchanged.
        (
            r"\overline{AB} + \underline{x}",
            [3.352_423, 0.883_320, 0.199_990],
        ),
        (
            r"\scriptstyle\underline{B}",
            [0.638_300, 0.478_334, 0.169_990],
        ),
        (
            r"\scriptstyle\underline{y}",
            [0.430_675, 0.301_389, 0.306_100],
        ),
        (
            r"\scriptscriptstyle\underline{B}",
            [0.541_846, 0.341_667, 0.121_422],
        ),
        (
            r"\scriptscriptstyle\underline{y}",
            [0.383_220, 0.215_277, 0.218_643],
        ),
        (
            r"\textstyle\underline{B}",
            [0.808_680, 0.683_331, 0.199_990],
        ),
        (
            r"\textstyle\underline{y}",
            [0.526_160, 0.430_554, 0.394_434],
        ),
        (
            r"\displaystyle\underline{B}",
            [0.808_680, 0.683_331, 0.199_990],
        ),
        (
            r"\displaystyle\underline{y}",
            [0.526_160, 0.430_554, 0.394_434],
        ),
    ] {
        let layout = e.typeset(source, Style::Display).unwrap();
        let actual = [layout.width, layout.height, layout.depth];
        for (got, want) in actual.into_iter().zip(expected) {
            assert!(
                (got - want).abs() < 1e-4,
                "{source}: {actual:?} vs {expected:?}"
            );
        }
    }
}

#[test]
fn primes_are_cmsy_prime_characters() {
    // Exact independently transcribed system TFM fix_words /2^20, not
    // the rounded AFM ink frame the former fixture asserted. Fresh TeX
    // boxes confirm these metrics (fm-7ojgl); retain the strict EPS.
    // `'` is `^{\prime}`, so `x''` superscripts two of them.
    const TEXT_ADVANCE: f64 = 0.274_999_618_530_273_44;
    const SCRIPT_ADVANCE: f64 = 0.329_365_730_285_644_53;
    const PRIME_HEIGHT: f64 = 0.555_557_250_976_562_5;
    let e = engine();
    let text = e.typeset(r"\prime", Style::Display).unwrap();
    assert!((text.width - TEXT_ADVANCE).abs() < EPS, "{text:?}");
    assert!((text.height - PRIME_HEIGHT).abs() < EPS && text.depth.abs() < EPS);
    let script_prime = 0.7 * SCRIPT_ADVANCE;
    for (src, n) in [("{}'", 1.0), ("{}''", 2.0)] {
        let l = e.typeset(src, Style::Display).unwrap();
        let w = n * script_prime + CM.script_space;
        assert!((l.width - w).abs() < EPS, "{src}: {} vs {w}", l.width);
        let h = CM.sup1 + 0.7 * PRIME_HEIGHT;
        assert!((l.height - h).abs() < EPS, "{src}: {} vs {h}", l.height);
    }
    let quote = e.typeset("f'(x)", Style::Display).unwrap();
    let explicit = e.typeset(r"f^\prime(x)", Style::Display).unwrap();
    assert_eq!(quote.glyphs.len(), explicit.glyphs.len());
    for (a, b) in quote.glyphs.iter().zip(&explicit.glyphs) {
        assert!((a.x - b.x).abs() < EPS && (a.y - b.y).abs() < EPS && a.size == b.size);
    }
}

#[test]
fn prime_boxes_match_independent_tex_across_styles_and_sizes() {
    // Real ruled-Reference TeX boxes at10pt (fm-7ojgl), including valid
    // size declarations in text mode around a math island. The raw
    // 21-case probe also retains three invalid in-math declarations.
    let e = engine();
    for (source, expected) in [
        (r"\prime", [0.274_998, 0.555_557, 0.0]),
        (r"\scriptstyle\prime", [0.230_556, 0.388_890, 0.0]),
        (r"\scriptscriptstyle\prime", [0.220_488, 0.277_779, 0.0]),
        ("{}'", [0.280_556, 0.801_782, 0.0]),
        ("{}''", [0.511_111, 0.801_782, 0.0]),
        ("x'", [0.852_083, 0.801_782, 0.0]),
        ("f'", [0.877_782, 0.801_782, 0.194_444]),
        ("x''", [1.082_639, 0.801_782, 0.0]),
        ("x'''", [1.313_194, 0.801_782, 0.0]),
        (r"f^{\prime}", [0.877_782, 0.801_782, 0.194_444]),
        (r"{\hat{x}}'", [0.852_083, 0.801_782, 0.0]),
        (r"{\bar{f}}'", [0.877_782, 0.831_667, 0.194_444]),
        (r"x^{y'}", [1.322_690, 0.992_450, 0.0]),
        (r"\scriptstyle x'", [0.723_961, 0.579_558, 0.0]),
        (r"\scriptscriptstyle x'", [0.666_324, 0.479_555, 0.0]),
        (r"\text{\tiny $\prime$}", [0.220_488, 0.277_779, 0.0]),
        (r"\text{\small $\prime$}", [0.251_389, 0.500_002, 0.0]),
        (r"\text{\Large $\prime$}", [0.395_999, 0.800_002, 0.0]),
    ] {
        let layout = e.typeset(source, Style::Display).unwrap();
        let actual = [layout.width, layout.height, layout.depth];
        for (got, want) in actual.into_iter().zip(expected) {
            assert!(
                (got - want).abs() < 1e-4,
                "{source}: {actual:?} vs {expected:?}"
            );
        }
    }
}

#[test]
fn braces_are_latex_brace_fills() {
    // LaTeX's \overbrace and \underbrace: the base in display style,
    // centered on a fill of four 0.45 em cmex pieces (never narrower than
    // 1.8 em), 3 pt from it, with 3 pt more past the fill's 0.12 em row.
    // Measured in LaTeX: \underbrace{x} is 18.00018 pt wide and 7.19997 pt
    // deeper than x. Those are four pieces' TFM widths, and 6 pt plus
    // \braceld's TFM height.
    let e = engine();
    let piece = fmd_math::tfm::CMEX10.char(0x7A).expect("braceld");
    let (fill_width, fill_extra) = (4.0 * piece.width, 0.6 + piece.height);
    let x = |layout: &Layout, ch: char| {
        layout
            .glyphs
            .iter()
            .find(|g| g.ch == ch)
            .unwrap_or_else(|| panic!("glyph {ch}"))
            .x
    };
    let frac = e.typeset(r"\frac{1}{6^2}", Style::Display).unwrap();
    let under = e
        .typeset(r"\underbrace{\frac{1}{6^2}}", Style::Display)
        .unwrap();
    assert!((under.width - fill_width).abs() < EPS, "{}", under.width);
    assert!((under.height - frac.height).abs() < EPS);
    assert!((under.depth - (frac.depth + fill_extra)).abs() < EPS);
    let centered = (fill_width - frac.width) / 2.0;
    assert!((x(&under, '1') - x(&frac, '1') - centered).abs() < EPS);
    let sum = e.typeset("a+b+c+d", Style::Display).unwrap();
    let over = e.typeset(r"\overbrace{a+b+c+d}", Style::Display).unwrap();
    assert!((over.width - sum.width).abs() < EPS);
    assert!((over.height - (sum.height + fill_extra)).abs() < EPS);
    assert!((over.depth - sum.depth).abs() < EPS);
    // \displaystyle resets the size: a braced base in a superscript is
    // full size.
    let scripted = e.typeset(r"x^{\overbrace{a}}", Style::Display).unwrap();
    assert!((glyph_size(&scripted, 'a') - 1.0).abs() < EPS);
}

#[test]
fn a_trailing_row_break_opens_no_empty_math_row() {
    // align* ends its last row at \crcr: `= x + \\` is one row, like `= x +`.
    // A TexText paragraph's trailing \\ still sets an empty line.
    let e = engine();
    let one = e.typeset(r"= x +", Style::Display).unwrap();
    let trailing = e.typeset(r"= x + \\", Style::Display).unwrap();
    assert!((trailing.height - one.height).abs() < EPS);
    assert!((trailing.depth - one.depth).abs() < EPS);
    assert!((trailing.width - one.width).abs() < EPS);
    let paragraph = e.typeset_text(r"a\\").unwrap();
    let line = e.typeset_text("a").unwrap();
    assert!(paragraph.depth > line.depth + CM.baseline_skip / 2.0);
}

#[test]
fn explicit_spaces_keep_inter_atom_spacing() {
    // tex.web §761 keeps the previous atom's type across glue and kerns:
    // `a\,=b` is `a=b` plus 3 mu, and `a\quad\Rightarrow\quad b` is
    // `a\Rightarrow b` plus two quads (plain TeX: 24.577 - 22.911 pt and
    // 45.133 - 25.133 pt at 10 pt).
    let e = engine();
    let width = |s: &str| e.typeset(s, Style::Text).unwrap().width;
    let mu = CM.quad / 18.0;
    assert!((width(r"a\,=b") - width("a=b") - 3.0 * mu).abs() < EPS);
    let quad = fmd_math::tfm::ECRM1000.param(6);
    assert!(
        (width(r"a\quad\Rightarrow\quad b") - width(r"a\Rightarrow b") - 2.0 * quad).abs() < EPS
    );
    // A kern still separates the characters, so no font kern applies
    // across it.
    assert!((width(r"A\!V") - width("A") - width("V") + 3.0 * mu).abs() < EPS);
}

/// fm-8zy2: in math mode TeX sets family 0 (digits, `+ = ( ) [ ]`) from
/// cmr10 at display and text size, and from cmr7 in scripts. CM Unicode's
/// Regular stands in for it in text mode only: its `+` and `=` carry 0.57
/// of cmr10's ink and its brackets 1.48.
#[test]
fn family_zero_is_cmr10_in_math_mode_and_cm_unicode_in_text_mode() {
    use fmd_math::faces::{FACE_REGULAR, FACE_ROMAN_7, FACE_ROMAN_10};
    let e = engine();
    let math = e.typeset("[1+2]=3", Style::Display).unwrap();
    assert_eq!(math.glyphs.len(), 7);
    for g in &math.glyphs {
        assert_eq!(g.face, FACE_ROMAN_10, "math-mode {:?}", g.ch);
    }
    let inline = e.typeset("[x]", Style::Text).unwrap();
    assert_eq!(inline.glyphs[0].face, FACE_ROMAN_10);
    let script = e.typeset("x^{[2]}", Style::Display).unwrap();
    for g in script.glyphs.iter().skip(1) {
        assert_eq!(g.face, FACE_ROMAN_7, "script {:?}", g.ch);
    }
    let text = e.typeset_text("[1+2]").unwrap();
    for g in &text.glyphs {
        assert_eq!(g.face, FACE_REGULAR, "text-mode {:?}", g.ch);
    }
}

/// franken_manim fm-zxxy: `\iint`, `\iiint` and `\iiiint` are amsmath's
/// `\MultiIntegral`, cmex's `\intop` repeated with `\intkern@` (−6 mu, a
/// further −3 mu in display style) after the Op–Op thin space, never a
/// fallback double-integral glyph. The scripts belong to the last `\intop`;
/// `\limits` makes the row one `\mathop` (amsmath's `\ints@b`). The row is
/// one drawn path of the `\intop` outlines, one part as the Reference's
/// wasysym `\iint` is one character. Real TeX's `\showbox` (TeX Live 2025,
/// article + amsmath), widths in pt at 10 pt; TeX truncates mu glue, so
/// widths get 1e-4 em.
#[test]
fn multiple_integrals_repeat_cmex_intop_as_amsmath_does() {
    use Style::{Display, Text};
    use fmd_math::paths::resolve_paths;
    let e = engine();
    let close_mu = |ours: f64, tex_pt: f64| (ours - tex_pt / 10.0).abs() < 1e-4;
    // One \intop's contours, and where its outline starts, in each style.
    let int = |style| {
        let contours = resolve_paths(&e, &e.typeset(r"\int", style).unwrap()).unwrap();
        (contours.len(), contours[0].start.0)
    };
    let row_starts = |l: &Layout, style| {
        let (n, _) = int(style);
        assert_eq!(l.paths.len(), 1, "one drawn row: {l:?}");
        let row = &l.paths[0];
        assert_eq!(row.contours.len() % n, 0, "{l:?}");
        row.contours
            .iter()
            .step_by(n)
            .map(|c| c.start.0)
            .collect::<Vec<f64>>()
    };
    for (tex, style, width, integrals) in [
        (r"\iint", Text, 11.66672, 2),
        (r"\iint", Display, 16.66678, 2),
        (r"\iiint", Text, 16.66676, 3),
        (r"\iiint", Display, 23.33354, 3),
        (r"\iiiint", Text, 21.66681, 4),
        (r"\iint_D f\,dA", Text, 38.98643, 2),
        (r"\iint_D f\,dA", Display, 41.4865, 2),
        (r"\iint\nolimits_D", Text, 16.97604, 2),
        (r"\iint\limits", Text, 11.66672, 2),
        (r"\iint\limits_D f", Text, 19.3056, 2),
        (r"\iint\limits_D f", Display, 24.30566, 2),
        (r"a\iint\limits_D b", Text, 24.57751, 2),
    ] {
        let l = e.typeset(tex, style).unwrap();
        assert!(
            close_mu(l.width, width),
            "{tex} in {style:?}: {} against TeX's {width} pt",
            l.width
        );
        assert!(l.glyphs.iter().all(|g| g.ch != '∫'), "{tex}: {l:?}");
        assert_eq!(row_starts(&l, style).len(), integrals, "{tex}: {l:?}");
    }
    // Text style: the second \intop starts 6.66667 - 3.33325 + 1.66663 pt
    // after the first, and the subscript tucks under the last one, at its
    // TFM width (4.72223 pt) with its italic correction removed.
    let l = e.typeset(r"\iint_D", Style::Text).unwrap();
    let xs = row_starts(&l, Text);
    assert!(close_mu(xs[1] - xs[0], 5.00005), "{xs:?}");
    assert!((xs[0] - int(Text).1).abs() < 1e-9, "{xs:?}");
    assert!(close_mu(glyph_x(&l, 'D'), 9.72228), "{l:?}");
    assert!((glyph_y(&l, 'D') + 0.355_56).abs() < 1e-5, "{l:?}");
    // Display style: cmex's display \intop, 10.00002 - 3.33325 - 1.66663 +
    // 1.66663 pt apart. With limits the row's 9 mu lead and the kern before
    // it cancel, so the first \intop stays where a lone one is drawn.
    let l = e.typeset(r"\iint\limits_D", Style::Display).unwrap();
    let xs = row_starts(&l, Display);
    assert!(close_mu(xs[1] - xs[0], 6.66677), "{xs:?}");
    assert!((xs[0] - int(Display).1).abs() < 1e-9, "{xs:?}");
    // The row carries the command's span.
    let l = e.typeset(r"x\iiint", Style::Text).unwrap();
    assert_eq!(
        (l.paths[0].span.start, l.paths[0].span.end),
        (1, 7),
        "{l:?}"
    );
    // \oiint has no amsmath definition and keeps its own glyph.
    let l = e.typeset(r"\oiint", Style::Text).unwrap();
    assert_eq!((l.glyphs.len(), l.paths.len()), (1, 0), "{l:?}");
}

/// franken_manim fm-zxxy: `\vec` is cmmi's own vector accent (0x7E) at the
/// style's optical size, unscaled at TeX's accent box, never a fallback
/// arrow fitted to it. TeX's `\showbox` of `$\vec{v}$` (article +
/// amsmath): the accent's char box shifted −0.3883 pt (v's skewchar kern,
/// 0.27779 pt, less half the 1.33218 pt by which the 6.53819 pt accent box
/// overhangs the 5.20601 pt v), on v's baseline; 5.20601 × 7.14444 pt.
#[test]
fn vector_accent_is_cmmis_own_character() {
    use fmd_math::faces::{FACE_MATH_ITALIC, FACE_MATH_ITALIC_7};
    let e = engine();
    let l = e.typeset(r"\vec{v}", Style::Text).unwrap();
    let accent = l.glyphs.iter().find(|g| g.ch == '\u{20D7}').unwrap();
    assert_eq!((accent.face, accent.size), (FACE_MATH_ITALIC, 1.0), "{l:?}");
    assert!((accent.x + 0.038_83).abs() < 1e-5, "{l:?}");
    assert!(accent.y.abs() < 1e-9, "{l:?}");
    assert!((l.width - 0.520_601).abs() < 1e-5 && (l.height - 0.714_444).abs() < 1e-5);
    // In a script the accent is cmmi7's, at the script size.
    let l = e.typeset(r"x^{\vec{v}}", Style::Text).unwrap();
    let accent = l.glyphs.iter().find(|g| g.ch == '\u{20D7}').unwrap();
    assert_eq!(
        (accent.face, accent.size),
        (FACE_MATH_ITALIC_7, 0.7),
        "{l:?}"
    );
}

/// franken_manim fm-zxxy: past cmex's 3.0 em successor a radical sign is
/// cmex's extensible (tex.web §713): the bottom piece 0x74, repeaters 0x75
/// and the top 0x76, never a drawn sign. TeX's `\showbox` of a display
/// `\sqrt` over a five-row matrix: 15.84148 pt wide; the sign's vbox
/// stacks the top piece (0.39998 + 5.60007 pt), seven repeaters (6.00006
/// pt each) and the bottom piece, its top on the overbar's top.
#[test]
fn tall_radicals_stack_cmex_pieces() {
    use fmd_math::faces::FACE_MATH_EXTENSION;
    let e = engine();
    let l = e
        .typeset(
            r"\sqrt{\begin{matrix}a\\b\\c\\d\\e\end{matrix}}",
            Style::Display,
        )
        .unwrap();
    assert!(l.paths.is_empty(), "no drawn sign: {l:?}");
    assert!((l.width - 1.584_148).abs() < 1e-5, "{l:?}");
    let sign: Vec<_> = l.glyphs.iter().filter(|g| g.ch == '√').collect();
    assert_eq!(sign.len(), 9, "{l:?}");
    assert!(
        sign.iter()
            .all(|g| g.face == FACE_MATH_EXTENSION && g.size == 1.0)
    );
    // Each piece's baseline sits its TFM height below the top of its slot:
    // the top piece's ink top (0.04 em above its baseline) on the rule's
    // top, then the recipe's 0.6 em slots (TeX's are the TFM's 0.600006),
    // the bottom piece's 1.8 em last.
    let rule = &l.rules[0];
    let top = rule.y + rule.height;
    assert!((sign[0].y - (top - 0.039_999)).abs() < 1e-5, "{l:?}");
    for (i, piece) in sign.iter().enumerate().skip(1) {
        #[allow(clippy::cast_precision_loss)]
        let slot_top = top - 0.6 * i as f64;
        assert!((piece.y - slot_top).abs() < 1e-5, "piece {i}: {l:?}");
    }
    assert!((rule.x - 1.055_559).abs() < 1e-5, "{rule:?}");
}

/// franken_manim fm-zxxy: an extensible delimiter stacks its pieces' char
/// boxes as tex.web §713's vlist does, so a piece whose TFM height is 0.04
/// em (the round and square tops and bottoms) hangs that far below the top
/// of its slot and the designed 0.02 em overlaps join every piece. TeX's
/// `\showbox` of a display `\left(` over a six-row matrix: the top piece's
/// box is 0.39998 + 17.60019 pt from the vbox top, then six repeaters of
/// 6.00006 pt, then the bottom piece's 0.39998 + 17.60019 pt.
#[test]
fn extensible_pieces_hang_by_their_tfm_height() {
    let e = engine();
    let l = e
        .typeset(
            r"\left(\begin{matrix}a\\b\\c\\d\\e\\f\end{matrix}\right.",
            Style::Display,
        )
        .unwrap();
    let pieces: Vec<f64> = l
        .glyphs
        .iter()
        .filter(|g| g.ch == '(')
        .map(|g| g.y)
        .collect();
    assert_eq!(pieces.len(), 8, "{l:?}");
    // The recipe's slots are 1.8 and 0.6 em (TeX's are the TFM's 1.800018
    // and 0.600006).
    assert!(
        (pieces[0] - (l.height - 0.039_999)).abs() < 1e-5,
        "{pieces:?}"
    );
    for (i, y) in pieces.iter().enumerate().skip(1) {
        #[allow(clippy::cast_precision_loss)]
        let slot_top = l.height - 1.8 - 0.6 * (i - 1) as f64;
        let height = if i == 7 { 0.039_999 } else { 0.0 };
        assert!(
            (y - (slot_top - height)).abs() < 1e-5,
            "piece {i}: {pieces:?}"
        );
    }
}

/// Boxes TeX's `\showbox` measures under the Reference's preamble (`[T1]{fontenc}`,
/// amsmath, amssymb): (source, style, width, height, depth) in points.
const TIER_TWO_BOXES: &[(&str, Style, f64, f64, f64)] = &[
    (r"\not=", Style::Text, 7.7778, 6.94444, 1.94443),
    (r"a\not=b", Style::Text, 22.91077, 6.94444, 1.94443),
    (r"\not<", Style::Text, 7.7778, 6.94444, 1.94443),
    (r"\not\subset", Style::Text, 7.7778, 6.94444, 1.94443),
    (r"\not\in", Style::Text, 6.66669, 6.94444, 1.94443),
    (r"\lvert x \rvert", Style::Text, 11.27084, 7.5, 2.5),
    (r"\lVert x \rVert", Style::Text, 15.7153, 7.5, 2.5),
    (r"a\hspace{1cm}b", Style::Text, 38.03029, 6.94444, 0.0),
    (r"a\hspace{-3pt}b", Style::Text, 6.57755, 6.94444, 0.0),
    (r"a\hspace*{5mm}b", Style::Text, 23.80391, 6.94444, 0.0),
    (r"a\hspace{0.5in}b", Style::Text, 45.71254, 6.94444, 0.0),
    (r"a\hspace{1cm}b", Style::Script, 36.30705, 4.8611, 0.0),
    (r"\boxed{x}", Style::Text, 12.51526, 7.70554, 3.4),
    (r"\boxed{E=mc^2}", Style::Text, 45.68535, 12.04002, 3.4),
    (
        r"\boxed{\frac{a}{b}}",
        Style::Text,
        14.48587,
        14.47061,
        10.2595,
    ),
    (r"x^{\boxed{y}}", Style::Text, 18.27686, 14.12636, 0.0),
    (r"\cfrac{1}{x}", Style::Text, 6.91527, 15.89993, 6.85951),
    (
        r"\cfrac{1}{1+\cfrac{1}{x}}",
        Style::Text,
        25.33742,
        15.89993,
        21.65938,
    ),
    (
        r"\cfrac{1}{1+\cfrac{1}{x}}",
        Style::Display,
        25.33742,
        15.89993,
        21.65938,
    ),
    (r"\cfrac[l]{1}{x+y}", Style::Text, 24.399, 15.89993, 8.80396),
    (r"\cfrac[r]{1}{x+y}", Style::Text, 24.399, 15.89993, 8.80396),
    (
        r"\genfrac{(}{)}{0pt}{}{n}{k}",
        Style::Text,
        14.11005,
        8.50005,
        3.50006,
    ),
    (
        r"\genfrac{}{}{}{0}{a}{b}",
        Style::Text,
        7.68588,
        11.07062,
        6.85951,
    ),
    (
        r"\genfrac{[}{]}{1pt}{1}{a}{b}",
        Style::Text,
        12.67102,
        8.50005,
        3.8611,
    ),
    (
        r"\genfrac{}{}{2pt}{}{a}{b}",
        Style::Text,
        6.73764,
        8.51389,
        5.3611,
    ),
    (
        r"\genfrac{\{}{\}}{0pt}{}{a}{b}",
        Style::Display,
        20.28592,
        14.5001,
        9.50012,
    ),
    (
        r"\mathchoice{a}{bb}{ccc}{dddd}",
        Style::Text,
        8.58331,
        6.94444,
        0.0,
    ),
    (
        r"\mathchoice{a}{bb}{ccc}{dddd}",
        Style::Display,
        5.28589,
        4.30554,
        0.0,
    ),
    (
        r"x^{\mathchoice{a}{bb}{ccc}{dddd}}",
        Style::Text,
        16.93651,
        6.6428,
        0.0,
    ),
    // \mathfrak: eufm10/7/5's TFM boxes, Letterlike capitals and digits
    // included; `+` stays cmr10's.
    (r"\mathfrak{g}", Style::Text, 5.037, 4.75342, 1.89056),
    (r"\mathfrak{su}(2)", Style::Text, 22.37648, 7.5, 2.5),
    (r"\mathfrak{A}", Style::Text, 7.17603, 6.91406, 0.0),
    (r"\mathfrak{CHIRZ}", Style::Text, 33.17397, 6.91406, 1.26038),
    (r"\mathfrak{0123}", Style::Text, 20.07239, 4.75342, 1.89056),
    (r"\mathfrak{a+b}", Style::Text, 22.35826, 6.91406, 0.83333),
    (r"x^{\mathfrak{p}}", Style::Text, 10.413, 7.29956, 0.0),
    (
        r"x_{\mathfrak{a}_{\mathfrak{b}}}",
        Style::Text,
        14.95657,
        4.30554,
        2.56544,
    ),
    (r"\mathfrak{g}", Style::Script, 4.22226, 3.33006, 1.32446),
    (
        r"\mathfrak{g}",
        Style::ScriptScript,
        3.98697,
        2.39014,
        0.95062,
    ),
    (
        r"\mathfrak{gl}_n",
        Style::Display,
        13.27567,
        6.91406,
        2.39055,
    ),
    (r"\mathfrak{xyz}", Style::Text, 12.78018, 4.75342, 1.89056),
    // cancel.sty: every slope of both branches, the 2/6/8 pt floors (the
    // empty body, the period), scripts and display style.
    (r"\cancel{x}", Style::Text, 5.71527, 7.15277, 0.34723),
    (r"\bcancel{x}", Style::Text, 5.71527, 7.15277, 0.34723),
    (r"\xcancel{x}", Style::Text, 5.71527, 7.15277, 0.34723),
    (r"\cancel{x+y}", Style::Text, 23.199, 5.09319, 1.94444),
    (
        r"\cancel{\frac{a}{b}}",
        Style::Text,
        6.73764,
        7.9464,
        3.44841,
    ),
    (
        r"\cancel{\frac{a}{b}}",
        Style::Display,
        7.68588,
        12.07054,
        6.85951,
    ),
    (r"\cancel{abcdefgh}", Style::Text, 40.62854, 7.8275, 1.94444),
    (r"a\cancel{b}c", Style::Text, 13.9051, 8.47221, 0.0),
    (r"x^{\cancel{2}}", Style::Text, 10.2014, 10.88448, 0.0),
    (r"\cancel{x}", Style::Script, 4.53473, 6.50694, 0.99306),
    (
        r"\cancel{x}",
        Style::ScriptScript,
        3.95836,
        6.07639,
        1.42361,
    ),
    (
        r"\cancel{\int_0^1 f}",
        Style::Text,
        18.79169,
        11.0657,
        3.5556,
    ),
    (r"\cancel{xy}", Style::Text, 10.97687, 4.42303, 1.94444),
    (r"\cancel{xyzwuv}", Style::Text, 34.42601, 4.21553, 1.94444),
    (r"\cancel{ab}", Style::Text, 9.57755, 7.81096, 0.0),
    (r"\cancel{mm}", Style::Text, 17.56027, 4.59778, 0.34723),
    (r"\cancel{\Bigg|}", Style::Text, 3.33333, 17.86, 12.50015),
    (r"\cancel{\big(}", Style::Text, 4.58336, 9.49998, 3.50006),
    (
        r"\cancel{\frac{abc}{d}}",
        Style::Text,
        13.82805,
        10.585,
        3.44841,
    ),
    (
        r"\cancel{\frac{ab}{cd}}",
        Style::Text,
        10.2543,
        9.795,
        3.44841,
    ),
    (
        r"\cancel{\sum_{i}^{n}}",
        Style::Display,
        14.44447,
        17.51263,
        12.79865,
    ),
    (r"\cancel{.}", Style::Text, 2.77779, 5.52777, 1.97223),
    (r"\cancel{}", Style::Text, 0.0, 5.0, 2.5),
    (r"\cancel{f}", Style::Text, 5.97226, 7.94, 1.94444),
    (r"2\cancel{x}^2", Style::Text, 15.20142, 8.14003, 0.34723),
    (
        r"\frac{\cancel{a}b}{\cancel{a}c}",
        Style::Text,
        10.31139,
        10.59998,
        5.59998,
    ),
    // amssymb: msam and msbm's TFM boxes, at the script cut too (fm-0byq).
    (r"\leqslant", Style::Text, 7.7778, 6.36665, 1.36665),
    (r"\square", Style::Text, 7.7778, 6.75, 0.0),
    (r"\therefore", Style::Text, 6.66669, 6.9224, 0.0),
    (r"\nleq", Style::Text, 7.7778, 7.93828, 3.02742),
    (r"\Bbbk", Style::Text, 5.55557, 6.88889, 0.0),
    (r"\thickapprox", Style::Text, 7.7778, 4.8312, 0.0),
    (
        r"\twoheadrightarrow",
        Style::Text,
        10.00002,
        5.22394,
        0.13539,
    ),
    (r"\curlyvee", Style::Text, 7.60416, 5.4986, 0.0),
    (r"\circledast", Style::Text, 7.7778, 5.81978, 0.81978),
    (r"\lessgtr", Style::Text, 7.7778, 6.75, 1.80801),
    (r"a\leqslant b", Style::Text, 22.91077, 6.94444, 1.36665),
    (r"\leqslant", Style::Script, 6.25002, 4.82222, 1.3297),
    (r"x^{\nleq}", Style::Text, 12.46529, 9.78418, 0.0),
    (r"\doublecap", Style::Text, 6.66669, 5.4986, 0.0),
    // LaTeX's fractions are Ords: no thin space against an Ord, an Open or a
    // Punct (fm-0byq's ink oracle rows).
    (r"a\frac{b}{c}d", Style::Text, 16.4645, 8.79842, 3.44841),
    (r"a\frac{b}{c}d", Style::Display, 17.2183, 13.70952, 6.85951),
    (r"a\binom{b}{c}d", Style::Display, 29.5406, 14.5001, 9.50012),
    (
        r"a\tbinom{b}{c}d",
        Style::Display,
        23.23122,
        9.2984,
        3.50006,
    ),
    (
        r"a\genfrac{[}{]}{0pt}{}{b}{c}d",
        Style::Display,
        25.37392,
        14.5001,
        9.50012,
    ),
    (
        r"a\cfrac{b}{c}d",
        Style::Display,
        16.01831,
        15.89993,
        6.85951,
    ),
    (r"a{b\choose c}d", Style::Display, 29.5406, 14.5001, 9.50012),
    (r"\frac{b}{c},", Style::Display, 9.50534, 13.70952, 6.85951),
    (
        r"\frac{b}{c}(x)",
        Style::Display,
        20.22063,
        13.70952,
        6.85951,
    ),
    // amsmath's \mod and \pmod in the Reference's align*, where \if@display
    // holds: an 18 mu kern, upright "mod" as one Ord (cmr10's o-d kern
    // inside) and 6 mu before the argument (fm-eo8l).
    (
        r"p \equiv 1 \mod 4",
        Style::Display,
        60.86424,
        6.94444,
        1.94444,
    ),
    (
        r"p \equiv 1 \mod 4",
        Style::Text,
        60.86424,
        6.94444,
        1.94444,
    ),
    (
        r"\times n \mod 256",
        Style::Display,
        61.27994,
        6.94444,
        0.83333,
    ),
    (
        r"x \equiv y \mod{n+1}",
        Style::Display,
        80.03433,
        6.94444,
        1.94444,
    ),
    (r"a \mod b", Style::Script, 33.96777, 4.8611, 0.0),
    (r"a \pmod b", Style::Display, 49.85509, 7.5, 2.5),
    (r"a \pmod{n}", Style::Text, 51.56578, 7.5, 2.5),
    // amsmath's \dddot and \ddddot: the base as an operator under a row of
    // text periods (fm-ew7o). The braces make the whole an Ord, so `a` and
    // `b` abut it, and the row keeps its text size in every style.
    (r"\dddot{x}", Style::Display, 9.99791, 5.90553, 0.0),
    (r"\ddddot{x}", Style::Display, 12.77501, 5.90553, 0.0),
    (r"\dddot{x}", Style::Text, 9.99791, 5.90553, 0.0),
    (r"\dddot{x}", Style::Script, 9.99791, 4.01387, 0.0),
    (r"\dddot{x}", Style::ScriptScript, 9.99791, 2.46704, 0.0),
    (r"a\dddot{x}b", Style::Display, 19.57545, 6.94444, 0.0),
    (r"\dddot{xyzw}", Style::Display, 23.49542, 7.30553, 1.94444),
    (r"\dddot{f}", Style::Display, 9.99791, 8.54443, 1.94444),
    (r"\ddddot{y}", Style::Text, 12.77501, 5.90553, 1.94444),
    (r"\dddot{x}^2", Style::Display, 14.48404, 8.64003, 0.0),
    (r"\dddot{x}_i", Style::Display, 13.3272, 5.90553, 1.49998),
    (r"\ddddot {x}(t) +", Style::Display, 31.94173, 7.5, 2.5),
];

/// amsmath's `\mod` and `\pod` read `\if@display` (fm-eo8l). The Tex surface
/// is the Reference's align*, a display (the rows above); a TexText `$…$`
/// island is inline math, with 12 mu and 8 mu; its `$$…$$` islands and
/// align* blocks are displays again. TeX boxes the inline forms
/// `$p \equiv 1 \mod 4$` at 57.53099 pt and `$a \pmod{n}$` at 46.01036 pt.
#[test]
fn mod_kerns_follow_amsmath_display_flag() {
    let e = engine();
    for (source, tex_pt) in [
        (r"$p \equiv 1 \mod 4$", 57.53099),
        (r"$a \pmod{n}$", 46.01036),
        (r"$$p \equiv 1 \mod 4$$", 60.86424),
        (r"\begin{align*}p \equiv 1 \mod 4\end{align*}", 60.86424),
        (r"$$a \pmod{n}$$", 51.56578),
    ] {
        let width = e
            .typeset_text(source)
            .unwrap_or_else(|err| panic!("{source}: {err}"))
            .width;
        assert!(
            (width - tex_pt / 10.0).abs() < 1e-4,
            "{source}: {width} em against TeX's {tex_pt} pt"
        );
    }
}

#[test]
fn tier_two_constructs_take_tex_boxes() {
    let e = engine();
    let close = |ours: f64, tex_pt: f64| (ours - tex_pt / 10.0).abs() < 1e-4;
    for &(tex, style, width, height, depth) in TIER_TWO_BOXES {
        let l = e
            .typeset(tex, style)
            .unwrap_or_else(|err| panic!("{tex}: {err}"));
        assert!(
            close(l.width, width) && close(l.height, height) && close(l.depth, depth),
            "{tex} in {style:?}: {} x ({} + {}) against TeX's {width} x ({height} + {depth}) pt",
            l.width,
            l.height,
            l.depth
        );
    }
}

/// Every on-curve point of a layout's paths, ems.
fn path_points(l: &fmd_math::Layout) -> Vec<(f64, f64)> {
    use fmd_math::PathSeg;
    let mut points = Vec::new();
    for path in &l.paths {
        for c in &path.contours {
            points.push(c.start);
            for s in &c.segments {
                match s {
                    PathSeg::Line { to } | PathSeg::Quad { to, .. } => points.push(*to),
                }
            }
        }
    }
    points
}

#[test]
fn cancel_strokes_run_corner_to_corner_of_tex_line_boxes() {
    // TeX's ink (dvisvgm of the line font's characters): \cancel{x}'s 5 x
    // 10 pt line starts 0.35764 pt right of the box and 2.84723 pt below the
    // baseline; \cancel{x+y}'s 25.199 x 6.29749 pt line starts 1 pt left of
    // it and 1.20430 pt below. The pen is round, 0.4 pt across, so each tip
    // overshoots its end by 0.2 pt along the line. The tolerance is the box
    // oracle's: the strokes follow the body's box, and `x+y` is 1.6e-5 em
    // wider than TeX's.
    let e = engine();
    for (tex, x0, y0, run, rise) in [
        (r"\cancel{x}", 0.35764, -2.84723, 5.0, 10.0_f64),
        (r"\cancel{x+y}", -1.0, -1.20430, 25.199, 6.29749),
    ] {
        let l = e.typeset(tex, Style::Text).unwrap();
        assert_eq!(l.paths.len(), 1, "{tex}");
        assert_eq!(l.paths[0].contours.len(), 1, "{tex}");
        let len = (run * run + rise * rise).sqrt();
        let (ux, uy) = (run / len, rise / len);
        let low = (0.1 * (x0 - 0.2 * ux), 0.1 * (y0 - 0.2 * uy));
        let high = (0.1 * (x0 + run + 0.2 * ux), 0.1 * (y0 + rise + 0.2 * uy));
        let points = path_points(&l);
        for tip in [low, high] {
            assert!(
                points
                    .iter()
                    .any(|p| (p.0 - tip.0).abs() < 1e-4 && (p.1 - tip.1).abs() < 1e-4),
                "{tex}: no cap apex at {tip:?} in {points:?}"
            );
        }
    }
    // \bcancel falls where \cancel rises; \xcancel is both, as one contour
    // with four caps.
    let rising = path_points(&e.typeset(r"\cancel{x}", Style::Text).unwrap());
    let falling = path_points(&e.typeset(r"\bcancel{x}", Style::Text).unwrap());
    // The 1:2 line's unit direction, and its ends: x from 0.035764 to
    // 0.535764 em, y from -0.284723 to 0.715277 em.
    let (ux, uy) = (1.0 / 5.0_f64.sqrt(), 2.0 / 5.0_f64.sqrt());
    let (left, right) = (0.035_764 - 0.02 * ux, 0.535_764 + 0.02 * ux);
    let (top, bottom) = (0.715_277 + 0.02 * uy, -0.284_723 - 0.02 * uy);
    let has = |ps: &[(f64, f64)], x: f64, y: f64| {
        ps.iter()
            .any(|p| (p.0 - x).abs() < 1e-5 && (p.1 - y).abs() < 1e-5)
    };
    assert!(
        has(&rising, right, top) && has(&rising, left, bottom),
        "{rising:?}"
    );
    assert!(
        has(&falling, left, top) && has(&falling, right, bottom),
        "{falling:?}"
    );
    assert!(!has(&rising, left, top) && !has(&falling, right, top));
    let cross = e.typeset(r"\xcancel{x}", Style::Text).unwrap();
    assert_eq!(cross.paths.len(), 1);
    assert_eq!(cross.paths[0].contours.len(), 1);
    // Per arm: out along an edge, a four-quad cap, back to a corner.
    assert_eq!(cross.paths[0].contours[0].segments.len(), 4 * (1 + 4 + 1));
    // The body keeps its own position: the x sits where it does alone.
    let alone = e.typeset("x", Style::Text).unwrap();
    let struck = e.typeset(r"\cancel{x}", Style::Text).unwrap();
    assert_eq!(struck.glyphs.len(), 1);
    assert!((struck.glyphs[0].x - alone.glyphs[0].x).abs() < 1e-12);
    assert!((struck.glyphs[0].y - alone.glyphs[0].y).abs() < 1e-12);
}

#[test]
fn cancel_strikes_text_bodies_and_reads_as_menclose() {
    use fmd_math::faces::{FACE_MATH_ITALIC, FACE_REGULAR};
    let e = engine();
    // TexText: cancel.sty strikes `\hbox{…}`, so the body stays text.
    let text = e.typeset_text(r"a \cancel{word} b").unwrap();
    assert_eq!(text.paths.len(), 1);
    assert!(
        text.glyphs.iter().all(|g| g.face == FACE_REGULAR),
        "{text:?}"
    );
    let math = e.typeset(r"\cancel{w}", Style::Text).unwrap();
    assert_eq!(math.glyphs[0].face, FACE_MATH_ITALIC);
    for (tex, notation) in [
        (r"\cancel{x}", "updiagonalstrike"),
        (r"\bcancel{x}", "downdiagonalstrike"),
        (r"\xcancel{x}", "updiagonalstrike downdiagonalstrike"),
    ] {
        let mathml = fmd_math::to_mathml(&fmd_math::parse(tex).unwrap(), false);
        assert!(
            mathml.contains(&format!(r#"<menclose notation="{notation}">"#)),
            "{tex}: {mathml}"
        );
    }
}

#[test]
fn not_is_cmsys_zero_width_slash_before_its_relation() {
    use fmd_math::faces::FACE_MATH_SYMBOLS;
    let e = engine();
    for tex in [r"\not=", r"\not<", r"\not\subset", r"\not\equiv"] {
        let l = e.typeset(tex, Style::Text).unwrap();
        assert_eq!(l.glyphs.len(), 2, "{tex}: the slash and the relation");
        let (slash, relation) = (&l.glyphs[0], &l.glyphs[1]);
        assert_eq!(slash.face, FACE_MATH_SYMBOLS, "{tex}");
        assert_eq!(slash.ch, '\u{0338}', "{tex}");
        // Rel–Rel: no space, so the slash sits at the relation's origin.
        assert!((slash.x - relation.x).abs() < 1e-12, "{tex}: {l:?}");
    }
    // `\neq` (one atom) and `\not=` (two) set the same ink in the same box.
    let neq = e.typeset(r"\neq", Style::Text).unwrap();
    let not = e.typeset(r"\not=", Style::Text).unwrap();
    assert!((neq.width - not.width).abs() < 1e-12);
    assert_eq!(
        neq.glyphs
            .iter()
            .map(|g| (g.face, g.gid))
            .collect::<Vec<_>>(),
        not.glyphs
            .iter()
            .map(|g| (g.face, g.gid))
            .collect::<Vec<_>>()
    );
}

#[test]
fn hspace_reads_ems_and_exes_from_the_text_font() {
    let e = engine();
    let qquad = e.typeset(r"a\qquad b", Style::Text).unwrap().width;
    let two_em = e.typeset(r"a\hspace{2em}b", Style::Text).unwrap().width;
    assert!((two_em - qquad).abs() < 1e-12, "2em is a \\qquad");
    // T1 ecrm1000's x-height, confirmed by the real TeX hspace capture,
    // in every math style.
    for style in [Style::Display, Style::Script] {
        let ab = e.typeset("ab", style).unwrap().width;
        let two_ex = e.typeset(r"a\hspace{2ex}b", style).unwrap().width;
        assert!((two_ex - ab - 2.0 * 0.430_450).abs() < 1e-5, "{style:?}");
    }
    // A declared size scales font-relative units, not fixed ones.
    let large_em = e.typeset(r"\large a\hspace{1em}b", Style::Text).unwrap();
    let large_ab = e.typeset(r"\large ab", Style::Text).unwrap().width;
    // Independently measured T1 EC12 hboxes: 22.91087 - 11.16374 pt.
    assert!((large_em.width - large_ab - 1.174_713).abs() < 1e-5);
    let large_cm = e.typeset(r"\large a\hspace{1cm}b", Style::Text).unwrap();
    assert!((large_cm.width - large_ab - 2.845_274).abs() < 1e-5);
}

#[test]
fn boxed_frames_a_display_formula_in_fixed_rules() {
    let e = engine();
    let l = e.typeset(r"\boxed{x}", Style::Text).unwrap();
    // Four 0.4 pt rules: top and bottom across the width, the sides between.
    assert_eq!(l.rules.len(), 4, "{l:?}");
    let x = l.glyphs.iter().find(|g| g.ch == 'x').unwrap();
    assert!((x.x - 0.34).abs() < 1e-12, "3 pt clear of a 0.4 pt rule");
    // Even as a script, the body is a display formula at full size.
    let script = e.typeset(r"x^{\boxed{y}}", Style::Text).unwrap();
    let y = script.glyphs.iter().find(|g| g.ch == 'y').unwrap();
    assert!((y.size - 1.0).abs() < 1e-12);
    // A size declaration scales the body, not the frame.
    let large = e.typeset(r"\large\boxed{x}", Style::Text).unwrap();
    let x = large.glyphs.iter().find(|g| g.ch == 'x').unwrap();
    assert!((x.x - 0.34).abs() < 1e-12);
    assert!(
        large
            .rules
            .iter()
            .all(|r| (r.width.min(r.height) - 0.04).abs() < 1e-12)
    );
}

#[test]
fn cfrac_positions_its_numerator() {
    let e = engine();
    let one_x = |tex: &str| {
        let l = e.typeset(tex, Style::Text).unwrap();
        l.glyphs.iter().find(|g| g.ch == '1').unwrap().x
    };
    // \nulldelimiterspace, then the numerator flush left; flush right, the
    // 23.199 pt fraction less the 5 pt "1"; centred between.
    assert!((one_x(r"\cfrac[l]{1}{x+y}") - 0.12).abs() < 1e-4);
    assert!((one_x(r"\cfrac[r]{1}{x+y}") - 1.9399).abs() < 1e-4);
    assert!((one_x(r"\cfrac{1}{x+y}") - (0.12 + 0.90995)).abs() < 1e-4);
}

#[test]
fn text_font_switches_take_their_faces() {
    use fmd_math::faces::{FACE_ITALIC, FACE_REGULAR, FACE_SANS, FACE_TYPEWRITER};
    let e = engine();
    for (tex, face) in [
        (r"\textit{word}", FACE_ITALIC),
        (r"\textrm{word}", FACE_REGULAR),
        (r"\textup{word}", FACE_REGULAR),
        (r"\textnormal{word}", FACE_REGULAR),
        (r"\texttt{word}", FACE_TYPEWRITER),
        (r"\textsf{word}", FACE_SANS),
    ] {
        for laid in [
            e.typeset(tex, Style::Text).unwrap(),
            e.typeset_text(&format!("a {tex}")).unwrap(),
        ] {
            let faces: Vec<_> = laid
                .glyphs
                .iter()
                .filter(|g| "word".contains(g.ch))
                .map(|g| g.face)
                .collect();
            assert_eq!(faces, vec![face; 4], "{tex}: {laid:?}");
        }
    }
}

#[test]
fn slanted_text_ends_with_its_italic_correction() {
    // LaTeX's \check@icr: `\textit{italic} text` sets `\kern 0.56512` after
    // the c (TeX's \showbox, T1 ecti1000; cmti10's is 0.056528 em), and
    // `\textit{x}.` none, the period being in \nocorrlist.
    let e = engine();
    let x_of = |l: &Layout, ch: char| l.glyphs.iter().find(|g| g.ch == ch).unwrap().x;
    let corrected = e.typeset_text(r"\textit{c}x").unwrap();
    let before_period = e.typeset_text(r"\textit{c}.x").unwrap();
    let c_end = x_of(&before_period, '.');
    assert!((x_of(&corrected, 'x') - c_end - 0.056_528).abs() < 1e-5);
    let emph = e.typeset_text(r"\emph{c}x").unwrap();
    assert!((x_of(&emph, 'x') - x_of(&corrected, 'x')).abs() < 1e-12);
    // A body ending in a space ends in glue, after which \/ adds nothing.
    let spaced = e.typeset_text(r"\textit{c }x").unwrap();
    let plain_space = e.typeset_text(r"\textit{c} x").unwrap();
    assert!(x_of(&plain_space, 'x') - x_of(&spaced, 'x') > 0.05);
    // In mathematics the correction is inside the text box: before a period too.
    let math = e.typeset(r"\textit{c}.", Style::Text).unwrap();
    assert!((x_of(&math, '.') - c_end - 0.056_528).abs() < 1e-5);
    // Upright switches add none.
    let upright = e.typeset_text(r"\textrm{c}x").unwrap();
    let upright_period = e.typeset_text(r"\textrm{c}.x").unwrap();
    assert!((x_of(&upright, 'x') - x_of(&upright_period, '.')).abs() < 1e-12);
}

#[test]
fn the_logos_are_the_kernels_constructions_in_computer_modern() {
    use fmd_math::faces::{FACE_ROMAN_7, FACE_ROMAN_10};
    // TeX's \showbox in Computer Modern (OT1; the Reference's T1 ecrm
    // differs by up to 6e-4 em): \TeX 18.6108 x (6.83331 + 2.15277) pt and
    // \LaTeX 25.66368 pt, its A raised 2.04997 pt.
    let e = engine();
    let close = |ours: f64, tex_pt: f64| (ours - tex_pt / 10.0).abs() < 1e-4;
    for (tex, width) in [(r"\TeX", 18.6108), (r"\LaTeX", 25.66368)] {
        for l in [
            e.typeset(tex, Style::Text).unwrap(),
            e.typeset_text(tex).unwrap(),
        ] {
            assert!(
                close(l.width, width) && close(l.height, 6.83331) && close(l.depth, 2.15277),
                "{tex}: {} x ({} + {})",
                l.width,
                l.height,
                l.depth
            );
        }
    }
    let l = e.typeset_text(r"\LaTeX").unwrap();
    let letters: Vec<_> = l.glyphs.iter().map(|g| (g.ch, g.face)).collect();
    assert_eq!(
        letters,
        [
            ('L', FACE_ROMAN_10),
            ('A', FACE_ROMAN_7),
            ('T', FACE_ROMAN_10),
            ('E', FACE_ROMAN_10),
            ('X', FACE_ROMAN_10)
        ]
    );
    let a = &l.glyphs[1];
    assert!((a.size - 0.7).abs() < 1e-12 && close(a.y, 2.04997), "{a:?}");
    let e_glyph = &l.glyphs[3];
    assert!(close(e_glyph.y, -2.15277), "{e_glyph:?}");
}

/// The lines of a laid plain-text paragraph, top to bottom: each line's
/// baseline, left edge, right edge (its last glyph's advance) and the
/// characters it sets. Every glyph of a plain-text line sits on its
/// baseline. Query each glyph's active EC metric; expected widths and
/// line contents below come from independent TeX, not this query.
fn text_lines(layout: &Layout) -> Vec<(f64, f64, f64, String)> {
    let faces = fmd_math::faces::FaceSet::bundled().unwrap();
    let mut lines: Vec<(f64, f64, f64, String)> = Vec::new();
    for g in &layout.glyphs {
        let (font, code) = faces.text_tfm(g.face, g.gid, 10.0 * g.size).unwrap();
        let advance = font.char(code).unwrap().width * g.size;
        match lines.iter_mut().find(|line| (line.0 - g.y).abs() < EPS) {
            Some(line) => {
                line.2 = g.x + advance;
                line.3.push(g.ch);
            }
            None => lines.push((g.y, g.x, g.x + advance, g.ch.to_string())),
        }
    }
    lines.sort_by(|a, b| b.0.total_cmp(&a.0));
    lines
}

#[test]
fn text_displays_end_the_line_and_keep_tex_vertical_skips() {
    let pack = fmd_math::MacroSet::pack("default").unwrap();
    // Independent article/10pt showbox: 12pt baselines plus 10pt above
    // and below a centred display. An explicit break before it keeps an
    // empty line; a break immediately after it does not add another one.
    for (source, math_y, after_y) in [
        (r"Before $$x$$ After", -2.2, -4.4),
        (r"Before\\$$x$$ After", -3.4, -5.6),
        (r"Before $$x$$\\After", -2.2, -4.4),
        (r"{Before $$x$$ After}", -2.2, -4.4),
        (r"\textbf{Before $$x$$ After}", -2.2, -4.4),
        (r"\small Before $$x$$ After", -1.95, -3.9),
    ] {
        let input = format!(r"\begin{{center}}{source}\end{{center}}");
        let layout = engine().typeset_text_with_macros(&input, &pack).unwrap();
        assert!(
            (glyph_y(&layout, 'x') - math_y).abs() < EPS,
            "{source}: {layout:?}"
        );
        assert!(
            (glyph_y(&layout, 'A') - after_y).abs() < EPS,
            "{source}: {layout:?}"
        );
        // The bundled bold face has a small existing glyph depth of its
        // own. Check the final line's bounds independently of that font
        // metric; the baseline assertions above remain TeX's exact ones.
        let ending = if source.starts_with(r"\textbf") {
            r"\textbf{After}"
        } else if source.starts_with(r"\small") {
            r"\small After"
        } else {
            "After"
        };
        let ending_depth = engine()
            .typeset_text_with_macros(ending, &pack)
            .unwrap()
            .depth;
        assert!(
            (layout.depth + after_y - ending_depth).abs() < EPS,
            "{source}: {layout:?}"
        );
    }
    for source in [r"Before $x$ After", r"Before $\displaystyle x$ After"] {
        let layout = engine().typeset_text_with_macros(source, &pack).unwrap();
        assert_eq!(glyph_y(&layout, 'x'), 0.0, "{source}");
        assert_eq!(glyph_y(&layout, 'A'), 0.0, "{source}");
    }
}

#[test]
fn scoped_displays_reset_line_state_through_trailing_declarations() {
    let pack = fmd_math::MacroSet::pack("default").unwrap();
    let e = engine();
    let control = e
        .typeset_text_with_macros(r"\begin{center}Before $$x$$After\end{center}", &pack)
        .unwrap();
    for body in [
        r"Before {$$x$$} After",
        r"Before {$$x$$\small} After",
        r"Before {{$$x$$\small}\color{red}} After",
        r"Before \textbf{$$x$$\small} After",
    ] {
        let source = format!(r"\begin{{center}}{body}\end{{center}}");
        let layout = e.typeset_text_with_macros(&source, &pack).unwrap();
        assert_eq!(
            (layout.width, layout.height, layout.depth),
            (control.width, control.height, control.depth),
            "{body}"
        );
        let glyphs = |layout: &Layout| {
            layout
                .glyphs
                .iter()
                .map(|glyph| {
                    (
                        glyph.ch,
                        glyph.x,
                        glyph.y,
                        glyph.size,
                        glyph.face,
                        glyph.color,
                    )
                })
                .collect::<Vec<_>>()
        };
        assert_eq!(glyphs(&layout), glyphs(&control), "{body}");
    }
}

#[test]
fn text_displays_use_short_skips_and_the_declared_line_width() {
    let pack = fmd_math::MacroSet::pack("default").unwrap();
    // A left-aligned short preceding line selects 0pt / 6pt skips.
    // The display is centred on the declared 345pt line, not on the
    // short text's natural width. Independent TeX showbox: x's advance
    // is 5.71527pt, so its left edge is 169.64236pt.
    let layout = engine()
        .typeset_text_with_macros(r"Before $$x$$ After", &pack)
        .unwrap();
    let x = layout.glyphs.iter().find(|g| g.ch == 'x').unwrap();
    assert!((x.x - 16.964236).abs() < 1e-5, "{x:?}");
    assert!((x.y + 1.2).abs() < EPS, "{x:?}");
    assert!((glyph_y(&layout, 'A') + 3.0).abs() < EPS);
}

#[test]
fn paragraph_display_grids_keep_first_and_last_row_baselines() {
    let pack = fmd_math::MacroSet::pack("default").unwrap();
    for environment in ["align*", "gather*"] {
        let source = format!(
            r"\begin{{center}}Before \begin{{{environment}}}x\\y\end{{{environment}}} After\end{{center}}"
        );
        let layout = engine().typeset_text_with_macros(&source, &pack).unwrap();
        assert!(
            (glyph_y(&layout, 'x') + 2.2).abs() < EPS,
            "{source}: {layout:?}"
        );
        assert!(
            (glyph_y(&layout, 'y') + 3.7).abs() < EPS,
            "{source}: {layout:?}"
        );
        assert!(
            (glyph_y(&layout, 'A') + 5.9).abs() < EPS,
            "{source}: {layout:?}"
        );
        assert!((layout.depth - 5.9).abs() < EPS, "{source}: {layout:?}");
    }
    let layout = engine()
        .typeset_text_with_macros(
            r"\begin{center}Before $$\frac{a}{b}$$ After\end{center}",
            &pack,
        )
        .unwrap();
    assert!((layout.depth - 4.674673).abs() < 1e-5, "{layout:?}");
}

#[test]
fn consecutive_displays_and_boxed_text_follow_tex_modes() {
    let pack = fmd_math::MacroSet::pack("default").unwrap();
    let layout = engine()
        .typeset_text_with_macros(r"\begin{center}Before $$x$$ $$y$$ After\end{center}", &pack)
        .unwrap();
    // No intervening paragraph line: the second display selects the
    // short skips even though surrounding ordinary lines are centred.
    assert!((glyph_y(&layout, 'x') + 2.2).abs() < EPS);
    assert!((glyph_y(&layout, 'y') + 4.4).abs() < EPS);
    assert!((glyph_y(&layout, 'A') + 6.2).abs() < EPS);
    for command in ["underline", "text", "cancel"] {
        // In restricted horizontal mode the two adjacent dollars enter
        // and immediately leave inline math. The intervening x is TEXT:
        // the independent underline showbox names T1/cmr/m/n/10, and its
        // width is 62.67914pt, not the 63.11792pt of the math-italic x.
        let boxed = format!(r"\{command}{{Before $$x$$ After}}");
        let plain = format!(r"\{command}{{Before x After}}");
        let boxed = engine().typeset_text_with_macros(&boxed, &pack).unwrap();
        let plain = engine().typeset_text_with_macros(&plain, &pack).unwrap();
        assert_eq!(
            (boxed.width, boxed.height, boxed.depth),
            (plain.width, plain.height, plain.depth),
            "{command}"
        );
        assert!(
            (boxed.width - 6.267914).abs() < 1e-5,
            "{command}: {boxed:?}"
        );
        assert_eq!(glyph_y(&boxed, 'A'), 0.0, "{command}");
        assert_eq!(
            boxed
                .glyphs
                .iter()
                .map(|g| (g.ch, g.x, g.y, g.size))
                .collect::<Vec<_>>(),
            plain
                .glyphs
                .iter()
                .map(|g| (g.ch, g.x, g.y, g.size))
                .collect::<Vec<_>>(),
            "{command}"
        );
    }
    // Text styling entered from mathematics is a box too, whereas the
    // same command in an ordinary paragraph permits a real display.
    let styled = engine()
        .typeset(r"\textbf{Before $$x$$ After}", Style::Display)
        .unwrap();
    let x = styled.glyphs.iter().find(|g| g.ch == 'x').unwrap();
    let after = styled.glyphs.iter().find(|g| g.ch == 'A').unwrap();
    assert_eq!(x.face, after.face);
    assert_eq!(x.y, 0.0);
    assert_eq!(after.y, 0.0);
}

#[test]
fn boxed_text_rejects_display_environments() {
    let pack = fmd_math::MacroSet::pack("default").unwrap();
    let e = engine();
    for environment in ["equation*", "gather*", "align*"] {
        let display = format!(r"\begin{{{environment}}}x\end{{{environment}}}");
        let check = |source: &str, result: Result<Layout, fmd_math::MathError>| {
            let error = result.expect_err("a display cannot escape a restricted text box");
            let expected_at = source.find(r"\begin").unwrap();
            assert!(
                matches!(&error, fmd_math::MathError::Malformed { what, at }
                    if *at == expected_at
                        && what.contains(&format!("env:{environment}"))
                        && what.contains("boxed text")),
                "{source}: {error:?}"
            );
        };
        for command in ["underline", "text", "cancel"] {
            let source = format!(r"Before \{command}{{inside {display} after}} After");
            check(&source, e.typeset_text_with_macros(&source, &pack));
        }
        let nested = format!(r"Before \underline{{\textit{{inside {display} after}}}} After");
        check(&nested, e.typeset_text_with_macros(&nested, &pack));
        // Font switches entered from mathematics are restricted text too.
        let math_text = format!(r"\textbf{{inside {display} after}}");
        check(
            &math_text,
            e.typeset_with_macros(&math_text, Style::Display, &pack),
        );
        // The same unboxed font switch in a paragraph permits displays.
        let unboxed = format!(r"Before \textbf{{inside {display} after}} After");
        let layout = e.typeset_text_with_macros(&unboxed, &pack).unwrap();
        assert!(glyph_y(&layout, 'x') < 0.0, "{unboxed}");
        assert!(glyph_y(&layout, 'A') < glyph_y(&layout, 'x'), "{unboxed}");
    }
    // Ordinary inline mathematics, including an inline grid, stays legal.
    for command in ["underline", "text", "cancel"] {
        let source = format!(r"\{command}{{Before $\begin{{pmatrix}}x\end{{pmatrix}}$ After}}");
        let layout = e.typeset_text_with_macros(&source, &pack).unwrap();
        assert!(layout.width.is_finite() && layout.width > 0.0, "{source}");
        assert!(
            layout.glyphs.iter().any(|glyph| glyph.ch == 'x'),
            "{source}"
        );
    }
}

#[test]
fn display_flow_keeps_slanted_text_corrections_and_punctuation_controls() {
    let pack = fmd_math::MacroSet::pack("default").unwrap();
    let e = engine();
    let x_of = |l: &Layout, ch: char| l.glyphs.iter().find(|g| g.ch == ch).unwrap().x;
    for command in ["textit", "emph"] {
        let corrected = format!(r"\begin{{center}}\{command}{{B $$y$$ c}}ZZZZ\end{{center}}");
        let period = format!(r"\begin{{center}}\{command}{{B $$y$$ c}}.ZZZZ\end{{center}}");
        let corrected = e.typeset_text_with_macros(&corrected, &pack).unwrap();
        let period = e.typeset_text_with_macros(&period, &pack).unwrap();
        // Independent showboxes give widths 29.60246pt and 31.81444pt.
        // Compare offsets within the final line: centering cannot hide
        // the closing italic kern, and a period must suppress that kern.
        let advance = x_of(&corrected, 'Z') - x_of(&corrected, 'c');
        let uncorrected = x_of(&period, '.') - x_of(&period, 'c');
        assert!(
            (advance - uncorrected - 0.056_528).abs() < 1e-5,
            "{command}"
        );
        assert!((corrected.width - 2.960_246).abs() < 1e-4, "{command}");
        assert!((period.width - 3.181_444).abs() < 1e-4, "{command}");
    }
}

#[test]
fn display_flow_preserves_scoped_color_size_and_source_spans() {
    let pack = fmd_math::MacroSet::pack("default").unwrap();
    let source = r"\begin{center}Before {\color{red}\small inner $$x$$ middle} After\end{center}";
    let layout = engine().typeset_text_with_macros(source, &pack).unwrap();
    let x = layout.glyphs.iter().find(|g| g.ch == 'x').unwrap();
    let after = layout.glyphs.iter().find(|g| g.ch == 'A').unwrap();
    assert_eq!(x.size, 0.9);
    let color = x.color.unwrap();
    assert_eq!((color.r, color.g, color.b), (1.0, 0.0, 0.0));
    assert_eq!(after.size, 1.0);
    assert!(after.color.is_none());
    assert_eq!(&source[x.span.start..x.span.end], "x");
    assert_eq!(&source[after.span.start..after.span.end], "A");
    // The size in force at the display governs its above skip; after
    // leaving the group, the resumed line uses the outer 12pt baseline.
    assert!((x.y + 1.95).abs() < EPS);
    assert!((after.y + 4.0).abs() < EPS);
}

#[test]
fn display_skips_and_grid_struts_follow_setspace() {
    let pack = fmd_math::MacroSet::pack("default").unwrap();
    // Independent default-template showboxes: setspace's everydisplay
    // stretches all four display skips, as well as the text baselines.
    for (body, depth) in [
        (r"\doublespacing Before $$x$$ After", 7.334833),
        (
            r"\doublespacing Before \begin{align*}x\\y\end{align*} After",
            9.635242,
        ),
        (r"\doublespacing \small Before $$x$$ After", 6.501327),
        (
            r"\begin{flushleft}\doublespacing Before $$x$$ After\end{flushleft}",
            5.001023,
        ),
    ] {
        let source = format!(r"\begin{{center}}{body}\end{{center}}");
        let layout = engine().typeset_text_with_macros(&source, &pack).unwrap();
        assert!((layout.depth - depth).abs() < 1e-4, "{body}: {layout:?}");
        assert!(
            (glyph_y(&layout, 'A') + depth).abs() < 1e-4,
            "{body}: {layout:?}"
        );
    }
}

/// TeX's lines of a `\centering` paragraph at 345 pt, as `(natural width
/// in pt, characters)` from `\showbox` and each line's `\unhcopy` (OT1
/// T1 EC with the template's font control in `article`), against fmd-math's
/// `center` block under the default pack.
fn assert_breaks_like_tex(text: &str, tex: &[(f64, &str)]) {
    use fmd_math::MacroSet;
    let pack = MacroSet::pack("fmd-math/pack/default").unwrap();
    let source = format!(r"\begin{{center}}{text}\end{{center}}");
    let layout = engine().typeset_text_with_macros(&source, &pack).unwrap();
    let lines = text_lines(&layout);
    assert_eq!(lines.len(), tex.len(), "{text}: {lines:?}");
    let centre = (lines[0].1 + lines[0].2) / 2.0;
    for (i, ((y, left, right, chars), (natural_pt, tex_chars))) in lines.iter().zip(tex).enumerate()
    {
        assert_eq!(
            chars,
            &tex_chars.replace([' ', '~'], ""),
            "{text}: line {i}"
        );
        // A line wider than 345 pt shrinks its glue to exactly 345 pt.
        let set = (natural_pt / 10.0).min(34.5);
        assert!(
            ((right - left) - set).abs() < 1e-4,
            "{text}: line {i} is {} wide, TeX {set}",
            right - left
        );
        // Centred lines share a centre, and stack \baselineskip apart.
        assert!(
            ((left + right) / 2.0 - centre).abs() < 1e-4,
            "{text}: line {i} centre"
        );
        assert!(
            (y - (lines[0].0 - 1.2 * i as f64)).abs() < 1e-9,
            "{text}: line {i} baseline {y}"
        );
    }
}

#[test]
fn a_paragraph_wider_than_its_line_breaks_where_tex_breaks_it() {
    // The default pack's line is the Reference's \linewidth, 345 pt. Every
    // ragged line has badness 0, so TeX's tie rule fills the first line;
    // the second line's width includes the extra space after "it." (TeX's
    // space factor).
    assert_breaks_like_tex(
        "Every paragraph that is wider than its line breaks where the line breaker of TeX would break it. The lines are centred, and they stack twelve points apart.",
        &[
            (
                332.39105,
                "Every paragraph that is wider than its line breaks where the line breaker of",
            ),
            (
                347.13745,
                "TeX would break it. The lines are centred, and they stack twelve points apart.",
            ),
        ],
    );
}

#[test]
fn a_line_shrinks_rather_than_breaking_when_tex_shrinks_it() {
    // With T1 metrics and disabled text kerns, the first line ends at
    // "of"; the second takes "TeX" and shrinks to the declared width.
    assert_breaks_like_tex(
        "Every paragraph that is wider than its line breaks where is line breaker of TeX would break it. The lines are centred, and they stack twelve points apart.",
        &[
            (
                325.22614,
                "Every paragraph that is wider than its line breaks where is line breaker of",
            ),
            (
                347.13745,
                "TeX would break it. The lines are centred, and they stack twelve points apart.",
            ),
        ],
    );
}

#[test]
fn a_tie_is_not_a_line_break() {
    assert_breaks_like_tex(
        "Words joined by a tie stay together on one line, even near the end of a long line of words that would otherwise break.",
        &[
            (
                332.89093,
                "Words joined by a tie stay together on one line, even near the end of a long",
            ),
            (181.81671, "line of words that would otherwise break."),
        ],
    );
    assert_breaks_like_tex(
        "Words joined by a tie stay together on one line, even near the end of a long~line of words that would otherwise break.",
        &[
            (
                311.22955,
                "Words joined by a tie stay together on one line, even near the end of a",
            ),
            (203.47809, "long~line of words that would otherwise break."),
        ],
    );
}

#[test]
fn bare_macro_sets_never_break_and_document_packs_have_the_reference_width() {
    use fmd_math::MacroSet;
    let text = r"\begin{center}Every paragraph that is wider than its line breaks where the line breaker of TeX would break it.\end{center}";
    let bare = MacroSet::new();
    assert_eq!(bare.line_width(), None);
    let layout = engine().typeset_text_with_macros(text, &bare).unwrap();
    assert_eq!(text_lines(&layout).len(), 1);
    // The pinned basic/empty standalone templates have the same 345pt
    // document width as default; only a bare generic set is unbounded.
    for pack in ["empty", "basic", "default"] {
        let pack = MacroSet::pack(pack).unwrap();
        assert_eq!(pack.line_width(), Some(34.5));
        let layout = engine().typeset_text_with_macros(text, &pack).unwrap();
        assert_eq!(text_lines(&layout).len(), 2);
    }
}

#[test]
fn interword_glue_follows_texs_space_factor() {
    // Independent T1 EC article hboxes with the template's ligature-disable
    // font control. Sentence space factors still widen interword glue;
    // capitals retain factor 1000 and closing punctuation passes it on.
    let e = engine();
    let reference = fmd_math::MacroSet::pack("default").unwrap();
    for (text, tex_pt) in [
        ("Yes. No", 35.60242),
        ("Yes No", 31.71448),
        ("U.S. Army", 46.96075),
        ("end.) Next", 47.76611),
        ("a, b; c: d", 39.99023),
        ("Why? Because! So", 82.95197),
        ("$x$. Next", 34.04169),
    ] {
        let width = e.typeset_text_with_macros(text, &reference).unwrap().width;
        assert!(
            (width - tex_pt / 10.0).abs() < 1e-4,
            "{text}: {width} vs TeX {}",
            tex_pt / 10.0
        );
    }
    // Across a font switch: the factor after `\emph{end.}` is the
    // period's, so the space after it is the widened one.
    let spaced = e.typeset_text(r"\emph{end.} Next").unwrap().width;
    let joined = e.typeset_text(r"\emph{end.}Next").unwrap().width;
    let font = &fmd_math::tfm::ECRM1000;
    assert!((spaced - joined - (font.space + font.param(7))).abs() < 1e-6);
}

#[test]
fn size_declarations_set_latexs_baselineskip() {
    // size10.clo: \Large is 14.4 pt on an 18 pt \baselineskip, \small 9 pt
    // on 11 pt; normal lines are 12 pt apart.
    let e = engine();
    for (text, pitch) in [
        (r"\Large ab\\cd", 1.8),
        (r"\small ab\\cd", 1.1),
        (r"ab\\cd", 1.2),
        (r"\begin{center}\Large ab\\cd\end{center}", 1.8),
    ] {
        let layout = e.typeset_text(text).unwrap();
        let lines = text_lines(&layout);
        assert_eq!(lines.len(), 2, "{text}");
        assert!(
            (lines[0].0 - lines[1].0 - pitch).abs() < 1e-9,
            "{text}: {lines:?}"
        );
    }
}

#[test]
fn a_blank_line_ends_the_paragraph() {
    // Blank lines and paragraph breaks create the same two centred lines.
    // Independent T1 boxes measure Special at 3.05481 em, first-line
    // height 0.688720 em, 1.2 em baseline pitch, last depth 0.194397 em.
    use fmd_math::MacroSet;
    let pack = MacroSet::pack("fmd-math/pack/default").unwrap();
    for body in [
        "Special\n\nproblem",
        "Special\n\n\n\nproblem\n\n",
        "Special \\\\\n\nproblem",
        "Special\\par problem",
    ] {
        let source = format!("\\begin{{center}}{body}\\end{{center}}");
        let layout = engine().typeset_text_with_macros(&source, &pack).unwrap();
        let lines = text_lines(&layout);
        assert_eq!(lines.len(), 2, "{body:?}: {lines:?}");
        assert_eq!(lines[0].3, "Special", "{body:?}");
        assert!(
            (lines[0].2 - lines[0].1 - 3.05481).abs() < 1e-4,
            "{body:?}: {lines:?}"
        );
        assert!(
            ((lines[0].1 + lines[0].2) - (lines[1].1 + lines[1].2)).abs() < 1e-4,
            "{body:?}"
        );
        // The block keeps its first line's baseline.
        assert!(
            (layout.height - 0.688720).abs() < 1e-4,
            "{body:?}: {}",
            layout.height
        );
        assert!(
            (layout.depth - 1.394397).abs() < 1e-4,
            "{body:?}: {}",
            layout.depth
        );
    }
    // In a box a blank line is only a space, as before.
    let boxed = engine()
        .typeset_text_with_macros("\\text{a\n\nb}", &pack)
        .unwrap();
    assert_eq!(text_lines(&boxed).len(), 1);
}

#[test]
fn a_line_break_drops_the_glue_before_it() {
    // The line break drops trailing glue before starting the next paragraph.
    // Independent T1 widths: 12.363647 and 11.430542 em. Both stay centred.
    use fmd_math::MacroSet;
    let pack = MacroSet::pack("fmd-math/pack/default").unwrap();
    let layout = engine()
        .typeset_text_with_macros(
            r"\begin{center}Weigh how likely the clue is \quad \\given that the guess holds\end{center}",
            &pack,
        )
        .unwrap();
    let lines = text_lines(&layout);
    assert_eq!(lines.len(), 2);
    assert!(((lines[0].1 + lines[0].2) - (lines[1].1 + lines[1].2)).abs() < 1e-4);
    assert_eq!(lines[0].3, "Weighhowlikelytheclueis");
    let (first, second) = (lines[0].2 - lines[0].1, lines[1].2 - lines[1].1);
    assert!((first - 12.363647).abs() < 1e-4, "{first}");
    assert!((second - 11.430542).abs() < 1e-4, "{second}");
}

#[test]
fn delimiters_walk_texs_size_chain() {
    // tex.web §707 under the Reference's preamble (amsfonts declares
    // cmex7 below 7.5 pt). TeX's \showbox picks: in a script, cmr10's paren
    // (the text size's small variant), then cmex7's 2.4 em paren past it,
    // then cmex7's extensible; in a scriptscript, cmr10's paren too; a
    // script \sqrt takes cmsy10's surd. Boxes from \sbox{$\displaystyle …$}
    // with amsmath's \if@display, in pt.
    use fmd_math::MacroSet;
    let e = engine();
    let default = MacroSet::pack("fmd-math/pack/default").unwrap();
    for (tex, w, h, d) in [
        (r"\left(\frac{1}{3}\right)", 22.1223, 14.5001, 9.50012),
        (r"x^{\left(\frac{1}{3}\right)}", 19.7959, 11.07639, 0.0),
        (r"x^{y^{\left(\frac{1}{3}\right)}}", 24.60265, 14.88239, 0.0),
        (
            r"x^{\left(\frac{\frac{1}{2}}{3}\right)}",
            26.27234,
            17.87654,
            0.0,
        ),
        (
            r"x^{\left(\frac{\frac{\frac{1}{2}}{3}}{4}\right)}",
            30.8251,
            26.27934,
            0.0,
        ),
        (
            r"x^{\left(\frac{1}{\frac{1}{\frac{1}{\frac{1}{2}}}}\right)}",
            33.2251,
            34.75319,
            0.0,
        ),
        (r"x^{\sqrt{\frac{1}{2}}}", 20.35146, 11.69139, 0.0),
        // A bar has no successors: its extensible covers every target past
        // the small variant, from cmex10 or cmex7.
        (r"\left|\frac{1}{2}\right|", 14.06667, 14.50012, 9.50012),
        (
            r"\left|\frac{\frac{1}{2}}{3}\right|",
            15.45277,
            15.79677,
            9.50012,
        ),
        (r"x^{\left|\frac{1}{2}\right|}", 17.57367, 11.07639, 0.0),
    ] {
        let l = e
            .typeset_with_macros(tex, Style::Display, &default)
            .unwrap();
        assert!(
            (l.width - w / 10.0).abs() < 1e-4
                && (l.height - h / 10.0).abs() < 1e-4
                && (l.depth - d / 10.0).abs() < 1e-4,
            "{tex}: ({}, {}, {}) against TeX's ({}, {}, {})",
            l.width,
            l.height,
            l.depth,
            w / 10.0,
            h / 10.0,
            d / 10.0
        );
    }
}

#[test]
fn over_arrow_boxes_follow_amsmath_alignment() {
    // Independent pre-edit ruled-Reference capture for fm-7xc7q: 36 cases,
    // input SHA19908e2a, retained with all54 previous controls. amsmath's
    // arrow fill contributes its own minimum width and row height; its
    // nucleus is a fresh formula, not a cramped primitive-accent nucleus.
    let e = engine();
    let reference = fmd_math::MacroSet::pack("default").unwrap();
    let mut mismatches = Vec::new();
    for (source, width, height, depth) in [
        (r"\overrightarrow{P}", 1.000023, 1.050206, 0.0),
        (r"\overrightarrow{}", 1.000023, 0.366875, 0.0),
        (r"\overrightarrow{8}", 1.000023, 1.011319, 0.0),
        (r"\overrightarrow{x}", 1.000023, 0.797429, 0.0),
        (r"\overrightarrow{y}", 1.000023, 0.797429, 0.194444),
        (r"\overrightarrow{f}", 1.000023, 1.061319, 0.194444),
        (r"\overrightarrow{{P}}", 1.000023, 1.050206, 0.0),
        (r"\overrightarrow{AB}", 1.558682, 1.050206, 0.0),
        (r"\overrightarrow{ABCD}", 3.200626, 1.050206, 0.0),
        (r"\overrightarrow{wxyz}", 2.349542, 0.797429, 0.194444),
        (
            r"\overrightarrow{\frac{1}{2}}",
            1.000023,
            1.687827,
            0.685951,
        ),
        (
            r"\overrightarrow{\frac{y}{\frac{1}{2}}}",
            1.000023,
            1.473936,
            1.079677,
        ),
        (r"\overrightarrow{\sqrt{x}}", 1.404863, 1.215967, 0.190904),
        (
            r"\overrightarrow{\overrightarrow{P}}",
            1.000023,
            1.417080,
            0.0,
        ),
        (r"\overrightarrow{P_2}", 1.090627, 1.050206, 0.149998),
        (r"\overrightarrow{x^2}", 1.020140, 1.230878, 0.0),
        (r"\overrightarrow{y}_{i}", 1.332951, 0.797429, 0.194444),
        (r"\overrightarrow{P}^{2}", 1.448636, 1.050206, 0.0),
        (r"\textstyle\overrightarrow{P}", 1.000023, 1.050206, 0.0),
        (
            r"\textstyle\overrightarrow{\frac{1}{2}}",
            1.000023,
            1.211717,
            0.344841,
        ),
        (r"\scriptstyle\overrightarrow{P}", 0.784882, 0.747190, 0.0),
        (r"\scriptstyle\overrightarrow{8}", 0.784882, 0.719966, 0.0),
        (r"\scriptstyle\overrightarrow{}", 0.784882, 0.268855, 0.0),
        (
            r"\scriptstyle\overrightarrow{ABCD}",
            2.537666,
            0.747190,
            0.0,
        ),
        (
            r"\scriptstyle\overrightarrow{\frac{1}{2}}",
            0.784882,
            0.859808,
            0.240950,
        ),
        (
            r"\scriptscriptstyle\overrightarrow{P}",
            0.649707,
            0.539870,
            0.0,
        ),
        (
            r"\scriptscriptstyle\overrightarrow{8}",
            0.649707,
            0.520424,
            0.0,
        ),
        (
            r"\scriptscriptstyle\overrightarrow{}",
            0.649707,
            0.198203,
            0.0,
        ),
        (
            r"\scriptscriptstyle\overrightarrow{ABCD}",
            2.164229,
            0.539870,
            0.0,
        ),
        (
            r"\scriptscriptstyle\overrightarrow{\frac{1}{2}}",
            0.649707,
            0.714156,
            0.265953,
        ),
        (r"\overleftarrow{P}", 1.000023, 1.050206, 0.0),
        (r"\overleftarrow{}", 1.000023, 0.366875, 0.0),
        (r"\overleftarrow{y}", 1.000023, 0.797429, 0.194444),
        (r"\overleftarrow{ABCD}", 3.200626, 1.050206, 0.0),
        (r"\overleftarrow{\frac{1}{2}}", 1.000023, 1.687827, 0.685951),
        (r"\scriptstyle\overleftarrow{P}", 0.784882, 0.747190, 0.0),
    ] {
        let layout = e
            .typeset_with_macros(source, Style::Display, &reference)
            .unwrap();
        if !((layout.width - width).abs() <= 1e-4
            && (layout.height - height).abs() <= 1e-4
            && (layout.depth - depth).abs() <= 1e-4)
        {
            mismatches.push(format!(
                "{source}: ({}, {}, {}) vs ({width}, {height}, {depth})",
                layout.width, layout.height, layout.depth
            ));
        }
    }
    assert!(mismatches.is_empty(), "{}", mismatches.join("\n"));
}

#[test]
fn over_arrow_outer_scripts_preserve_explicit_grouping() {
    // Independent post-failure TeX showbox traces distinguish scripts on
    // mathpalette's following empty noad from scripts on a grouped box.
    // Keep the original36-case oracle and its tolerance unchanged.
    let e = engine();
    let reference = fmd_math::MacroSet::pack("default").unwrap();
    for (source, width, height, depth) in [
        (r"\overrightarrow{P}^{2}", 1.448_636, 1.050_206, 0.0),
        (r"{\overrightarrow{P}}^{2}", 1.448_636, 1.254_097, 0.0),
        (r"\overrightarrow{y}_{i}", 1.332_951, 0.797_429, 0.194_444),
        (r"{\overrightarrow{y}}_{i}", 1.332_951, 0.797_429, 0.244_443),
        (r"\overleftarrow{P}^{2}", 1.448_636, 1.050_206, 0.0),
        (r"\overrightarrow{}^{2}", 1.448_636, 0.864_003, 0.0),
        (r"\overrightarrow{P}{}^{2}", 1.448_636, 1.050_206, 0.0),
        (r"\mathchoice{P}{P}{P}{P}^{2}", 1.229_517, 0.864_003, 0.0),
        (r"{\mathchoice{P}{P}{P}{P}}^{2}", 1.229_517, 0.887_222, 0.0),
    ] {
        let layout = e
            .typeset_with_macros(source, Style::Display, &reference)
            .unwrap();
        assert!(
            (layout.width - width).abs() <= 1e-4
                && (layout.height - height).abs() <= 1e-4
                && (layout.depth - depth).abs() <= 1e-4,
            "{source}: ({}, {}, {}) vs ({width}, {height}, {depth})",
            layout.width,
            layout.height,
            layout.depth
        );
    }
}

#[test]
fn over_arrow_nuclei_restart_the_math_alphabet() {
    // Independent pre-fix Reference capture: outer math alphabets stop at
    // amsmath's fresh $style nucleus$ formula. Explicit inner alphabets
    // still apply. Primitive wide hats retain the surrounding alphabet.
    let e = engine();
    let reference = fmd_math::MacroSet::pack("default").unwrap();
    for (source, width, height) in [
        (r"\mathrm{\overrightarrow{ABCD}}", 3.200_626, 1.050_206),
        (r"\overrightarrow{\mathrm{ABCD}}", 2.944_450, 1.050_206),
        (r"\mathrm{\overleftarrow{ABCD}}", 3.200_626, 1.050_206),
        (r"\overleftarrow{\mathrm{ABCD}}", 2.944_450, 1.050_206),
        (
            r"\scriptstyle\mathrm{\overrightarrow{ABCD}}",
            2.537_666,
            0.747_190,
        ),
        (
            r"\scriptstyle\overrightarrow{\mathrm{ABCD}}",
            2.322_234,
            0.747_190,
        ),
        (
            r"\scriptscriptstyle\mathrm{\overrightarrow{ABCD}}",
            2.164_229,
            0.539_870,
        ),
        (
            r"\scriptscriptstyle\overrightarrow{\mathrm{ABCD}}",
            1.938_919,
            0.538_083,
        ),
        (r"\mathrm{\widehat{ABCD}}", 2.944_450, 1.045_372),
        (r"\mathrm{ABCD}", 2.944_450, 0.683_331),
        (r"\overrightarrow{ABCD}", 3.200_626, 1.050_206),
    ] {
        let layout = e
            .typeset_with_macros(source, Style::Display, &reference)
            .unwrap();
        assert!(
            (layout.width - width).abs() <= 1e-4
                && (layout.height - height).abs() <= 1e-4
                && layout.depth.abs() <= 1e-4,
            "{source}: ({}, {}, {}) vs ({width}, {height}, 0)",
            layout.width,
            layout.height,
            layout.depth
        );
    }
}

#[test]
fn over_arrow_nuclei_are_centered_without_changing_their_baseline() {
    let e = engine();
    let reference = fmd_math::MacroSet::pack("default").unwrap();
    for style in [
        Style::Display,
        Style::Text,
        Style::Script,
        Style::ScriptScript,
    ] {
        let plain = e.typeset_with_macros("P", style, &reference).unwrap();
        let plain_glyph = plain.glyphs.iter().find(|glyph| glyph.ch == 'P').unwrap();
        for source in [r"\overrightarrow{P}", r"\overleftarrow{P}"] {
            let arrow = e.typeset_with_macros(source, style, &reference).unwrap();
            let nucleus = arrow.glyphs.iter().find(|glyph| glyph.ch == 'P').unwrap();
            let centered_x = plain_glyph.x + (arrow.width - plain.width) / 2.0;
            assert!((nucleus.x - centered_x).abs() <= EPS, "{source}: {style:?}");
            assert_eq!(nucleus.y.to_bits(), plain_glyph.y.to_bits());
            assert_eq!(nucleus.size.to_bits(), plain_glyph.size.to_bits());
            assert_eq!(nucleus.face, plain_glyph.face);
        }
    }
}

#[test]
fn script_greek_capitals_take_cmr7_and_cmr5() {
    // cmr's upright Greek capitals (codes 0-10) at script and scriptscript
    // size are cmr7's and cmr5's own designs, as digits are; scaling cmr10's
    // set them 0.0736 em narrow in a script (franken_manim
    // fm-script-greek-capitals-r12-nbcf, found by its grammar differential).
    // TeX Live's \showbox under the Reference preamble (display style,
    // amsmath's \if@display), in pt.
    use fmd_math::MacroSet;
    let e = engine();
    let default = MacroSet::pack("fmd-math/pack/default").unwrap();
    for (tex, w, h, d) in [
        (r"C_{\Delta}", 14.21669, 6.83331, 1.49998),
        (r"{\scriptstyle \Delta \, 1}", 11.92134, 4.78334, 0.0),
        (r"{\scriptscriptstyle \Delta}", 5.4862, 3.3988, 0.0),
        (r"\sqrt[\Omega]{1}", 15.3474, 9.56036, 0.8396),
        (r"\tbinom{2}{\Omega - 3}", 25.11122, 8.94841, 4.28175),
        (r"\tfrac{\Delta}{b}", 8.96947, 8.72066, 3.44841),
    ] {
        let l = e
            .typeset_with_macros(tex, Style::Display, &default)
            .unwrap();
        assert!(
            (l.width - w / 10.0).abs() < 1e-4
                && (l.height - h / 10.0).abs() < 1e-4
                && (l.depth - d / 10.0).abs() < 1e-4,
            "{tex}: ({}, {}, {}) against TeX's ({}, {}, {})",
            l.width,
            l.height,
            l.depth,
            w / 10.0,
            h / 10.0,
            d / 10.0
        );
    }
}
