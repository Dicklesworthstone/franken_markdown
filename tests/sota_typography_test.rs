//! Integration test suite for the SOTA World-Class Typography & Visual Optimization Suite.

use franken_markdown::layout::{
    ColumnBadnessCurve, ContinuousHzExpansion, FontSize, LayoutUnit, OpticalKerningConfig,
    ParagraphCandidates, ParagraphVariant, RaggedConfig, SpaceCoordinate, VerticalSpring,
    compute_drop_cap_profile, compute_optical_kerning, compute_ragged_silhouette_demerits,
    compute_river_penalty, detect_white_rivers, snap_blocks_to_baseline_grid,
    solve_2d_optimal_pagination, solve_convex_table_widths,
};

#[test]
fn sota_track1_ragged_silhouette_smoothness() {
    let config = RaggedConfig::default();
    let measure = LayoutUnit::from_points(320);

    // Progressive smooth right edge (natural variation within 85-95%)
    let line1 = LayoutUnit::from_points(290);
    let line2 = LayoutUnit::from_points(280);
    let line3 = LayoutUnit::from_points(295);
    let demerits_smooth = compute_ragged_silhouette_demerits(
        line2,
        Some(line1),
        Some(line3),
        measure,
        false,
        &config,
    );

    // Sawtooth pattern: line 2 is drastically short between two long lines
    let line2_sawtooth = LayoutUnit::from_points(210);
    let demerits_sawtooth = compute_ragged_silhouette_demerits(
        line2_sawtooth,
        Some(line1),
        Some(line3),
        measure,
        false,
        &config,
    );

    // Hyphenated line
    let demerits_hyphen =
        compute_ragged_silhouette_demerits(line2, Some(line1), Some(line3), measure, true, &config);

    assert!(
        demerits_sawtooth > demerits_smooth,
        "Sawtooth inflection must be heavily penalized over smooth silhouette"
    );
    assert!(
        demerits_hyphen > demerits_smooth + 4000,
        "Hyphenation in ragged text must carry severe penalty"
    );
}

#[test]
fn sota_track2_white_river_penalty_and_detection() {
    let spaces_line1 = vec![
        SpaceCoordinate {
            x: LayoutUnit::from_points(60),
            width: LayoutUnit::from_points(4),
        },
        SpaceCoordinate {
            x: LayoutUnit::from_points(150),
            width: LayoutUnit::from_points(4),
        },
    ];

    let spaces_line2_aligned = vec![
        SpaceCoordinate {
            x: LayoutUnit::from_points(61), // Vertically aligned with Line 1
            width: LayoutUnit::from_points(4),
        },
        SpaceCoordinate {
            x: LayoutUnit::from_points(190),
            width: LayoutUnit::from_points(4),
        },
    ];

    let spaces_line2_dispersed = vec![
        SpaceCoordinate {
            x: LayoutUnit::from_points(95), // Dispersed away from Line 1
            width: LayoutUnit::from_points(4),
        },
        SpaceCoordinate {
            x: LayoutUnit::from_points(190),
            width: LayoutUnit::from_points(4),
        },
    ];

    let penalty_aligned = compute_river_penalty(
        &spaces_line2_aligned,
        &spaces_line1,
        LayoutUnit::from_points(4),
    );
    let penalty_dispersed = compute_river_penalty(
        &spaces_line2_dispersed,
        &spaces_line1,
        LayoutUnit::from_points(4),
    );

    assert!(
        penalty_aligned > 0,
        "Aligned spaces must trigger positive river penalty"
    );
    assert_eq!(
        penalty_dispersed, 0,
        "Dispersed spaces must trigger zero river penalty"
    );

    // Multi-line river detection
    let spaces_line3_aligned = vec![SpaceCoordinate {
        x: LayoutUnit::from_points(60),
        width: LayoutUnit::from_points(4),
    }];

    let doc_spaces = vec![spaces_line1, spaces_line2_aligned, spaces_line3_aligned];
    let findings = detect_white_rivers(&doc_spaces, LayoutUnit::from_points(3), 3);
    assert_eq!(findings.len(), 1, "Must detect 3-line continuous river");
    assert_eq!(findings[0].line_count, 3);
}

#[test]
fn sota_track3_constrained_convex_table_allocator() {
    // 3-column table:
    // Col 0: ID (narrow, badness drops fast)
    // Col 1: Name (medium)
    // Col 2: Long Description (wide narrative, badness very high when constrained)
    let col0 = ColumnBadnessCurve {
        column_index: 0,
        min_width: LayoutUnit::from_points(30),
        max_width: LayoutUnit::from_points(80),
        samples: vec![
            (LayoutUnit::from_points(30), 200),
            (LayoutUnit::from_points(50), 0),
            (LayoutUnit::from_points(80), 0),
        ],
    };

    let col1 = ColumnBadnessCurve {
        column_index: 1,
        min_width: LayoutUnit::from_points(60),
        max_width: LayoutUnit::from_points(150),
        samples: vec![
            (LayoutUnit::from_points(60), 800),
            (LayoutUnit::from_points(100), 50),
            (LayoutUnit::from_points(150), 0),
        ],
    };

    let col2 = ColumnBadnessCurve {
        column_index: 2,
        min_width: LayoutUnit::from_points(100),
        max_width: LayoutUnit::from_points(400),
        samples: vec![
            (LayoutUnit::from_points(100), 10_000),
            (LayoutUnit::from_points(200), 1_500),
            (LayoutUnit::from_points(300), 100),
            (LayoutUnit::from_points(400), 0),
        ],
    };

    let total_table_width = LayoutUnit::from_points(450);
    let widths = solve_convex_table_widths(&[col0, col1, col2], total_table_width);

    assert_eq!(widths.len(), 3);
    let total_allocated = widths[0] + widths[1] + widths[2];
    assert_eq!(
        total_allocated.milli_points(),
        total_table_width.milli_points(),
        "Table allocator must satisfy total width conservation exactly"
    );

    // Narrative description column must receive majority of width budget
    assert!(
        widths[2] > widths[1] && widths[1] > widths[0],
        "Column widths must reflect optimal badness reduction hierarchy"
    );
}

#[test]
fn sota_track4_continuous_hz_microtypography() {
    let hz = ContinuousHzExpansion::CONSERVATIVE;
    let base_width = LayoutUnit::from_points(200);

    // Expand by +15 variation units (+1.5%)
    let expanded_delta = hz.compute_width_delta(40, base_width, 15);
    assert_eq!(
        expanded_delta.milli_points(),
        3_000,
        "1.5% expansion on 200 pt must equal exactly +3 pt (3,000 milli-points)"
    );

    // Condense by -15 variation units (-1.5%)
    let condensed_delta = hz.compute_width_delta(40, base_width, -15);
    assert_eq!(
        condensed_delta.milli_points(),
        -3_000,
        "1.5% compression on 200 pt must equal exactly -3 pt (-3,000 milli-points)"
    );

    // Coordinate clamping beyond bounds
    let clamped_delta = hz.compute_width_delta(40, base_width, 50);
    assert_eq!(
        clamped_delta.milli_points(),
        3_000,
        "Excess variation coordinates must clamp to max budget"
    );
}

#[test]
fn sota_track5_gap_area_quadrature_optical_kerning() {
    let config = OpticalKerningConfig::default();

    // Two rectangular bounding silhouettes with 10 pt gap
    let left_contour = vec![
        (LayoutUnit::from_points(0), LayoutUnit::from_points(10)),
        (LayoutUnit::from_points(10), LayoutUnit::from_points(10)),
    ];
    let right_contour = vec![
        (LayoutUnit::from_points(0), LayoutUnit::from_points(0)),
        (LayoutUnit::from_points(10), LayoutUnit::from_points(0)),
    ];

    let natural_advance = LayoutUnit::from_points(15);
    let target_area = LayoutUnit::from_points(5);

    let adjustment = compute_optical_kerning(
        &left_contour,
        &right_contour,
        natural_advance,
        target_area,
        &config,
    );

    // Target area is 5 pt, current gap is 5 pt (15 advance - 10 right edge), so adjustment is 0
    assert_eq!(
        adjustment.milli_points(),
        0,
        "When current gap matches target area, adjustment must be 0"
    );

    // When target area is 3 pt (tighter), adjustment should be negative
    let tight_target = LayoutUnit::from_points(3);
    let tight_adjustment = compute_optical_kerning(
        &left_contour,
        &right_contour,
        natural_advance,
        tight_target,
        &config,
    );
    assert_eq!(
        tight_adjustment.milli_points(),
        -2_000,
        "Adjustment must shift glyph closer by -2 pt"
    );
}

#[test]
fn sota_track6_organic_drop_cap_profiles() {
    let font_size = FontSize::from_points(36);
    let line_height = LayoutUnit::from_points(14);
    let gap = LayoutUnit::from_points(4);

    let profile_w = compute_drop_cap_profile('W', font_size, line_height, 3, gap);
    let profile_a = compute_drop_cap_profile('A', font_size, line_height, 3, gap);
    let profile_i = compute_drop_cap_profile('I', font_size, line_height, 3, gap);

    assert_eq!(profile_w.total_lines, 3);
    assert_eq!(profile_a.total_lines, 3);
    assert_eq!(profile_i.total_lines, 3);

    // Wide 'W' occupies significantly more width reduction than slim 'I'
    assert!(
        profile_w.line_widths_reduction[0].milli_points()
            > profile_i.line_widths_reduction[0].milli_points() * 2,
        "'W' drop cap must allocate more space than 'I'"
    );

    // 'A' drop cap tapers (line 0 narrower at top than line 2 at bottom)
    assert!(
        profile_a.line_widths_reduction[0] < profile_a.line_widths_reduction[2],
        "'A' drop cap must taper organically wider at the bottom"
    );
}

#[test]
fn sota_track7_elastic_baseline_grid_synchronization() {
    let block_heights = vec![
        LayoutUnit::from_points(28), // 2 lines of prose (2 * 14 pt)
        LayoutUnit::from_points(18), // Heading (fractional 18 pt)
        LayoutUnit::from_points(42), // 3 lines of prose (3 * 14 pt)
    ];

    let springs = vec![
        VerticalSpring {
            natural_height: LayoutUnit::from_points(8),
            min_height: LayoutUnit::from_points(2),
            max_height: LayoutUnit::from_points(20),
            stiffness: 10,
        },
        VerticalSpring {
            natural_height: LayoutUnit::from_points(10),
            min_height: LayoutUnit::from_points(2),
            max_height: LayoutUnit::from_points(20),
            stiffness: 5,
        },
    ];

    let grid = LayoutUnit::from_points(14);
    let resolved = snap_blocks_to_baseline_grid(&block_heights, &springs, grid);

    assert_eq!(resolved.len(), 2);

    // Compute cumulative baseline positions
    let y1 = block_heights[0] + resolved[0];
    let y2 = y1 + block_heights[1] + resolved[1];

    assert_eq!(
        y1.milli_points() % grid.milli_points(),
        0,
        "Baseline 1 must land exactly on grid pitch"
    );
    assert_eq!(
        y2.milli_points() % grid.milli_points(),
        0,
        "Baseline 2 must land exactly on grid pitch"
    );
}

#[test]
fn sota_track8_2d_joint_pagination_widow_orphan_eradication() {
    let p1 = ParagraphCandidates {
        variants: vec![
            ParagraphVariant {
                line_count: 3, // Variant L-1 (tighter)
                demerits: 200,
                lines: Vec::new(),
            },
            ParagraphVariant {
                line_count: 4, // Variant L (natural)
                demerits: 0,
                lines: Vec::new(),
            },
        ],
    };

    let p2 = ParagraphCandidates {
        variants: vec![
            ParagraphVariant {
                line_count: 4, // Variant L-1
                demerits: 150,
                lines: Vec::new(),
            },
            ParagraphVariant {
                line_count: 5, // Variant L (natural)
                demerits: 0,
                lines: Vec::new(),
            },
        ],
    };

    // Page capacity is 8 lines.
    // If p1 takes 4 lines and p2 takes 5 lines = 9 lines total:
    // A greedy page break would put 4 lines of p1 + 4 lines of p2 on page 1, and 1 line of p2 on page 2 (WIDOW!).
    // The 2D joint solver will choose p1 (4) + p2 variant L-1 (4) = 8 lines, fitting page 1 perfectly without a widow!
    let breaks = solve_2d_optimal_pagination(&[p1, p2], 8, 100_000, 100_000);

    // No breaks needed on page 1 because p2 variant L-1 fit completely within 8 lines!
    assert_eq!(
        breaks.len(),
        0,
        "2D joint pagination must compress paragraph to fit page and eradicate widow"
    );
}

#[test]
fn sota_edge_cases_and_robustness_gauntlet() {
    // 1. Continuous Hz Expansion with arbitrary coordinates and bounds
    let custom_hz = ContinuousHzExpansion {
        min_coord: -50,
        max_coord: 50,
        delta_width_per_glyph_permille: 30, // 3.0%
    };
    let width = LayoutUnit::from_points(100);
    let delta_max = custom_hz.compute_width_delta(10, width, 50);
    assert_eq!(
        delta_max.milli_points(),
        3_000,
        "3% expansion on 100 pt = 3 pt"
    );
    let delta_clamped = custom_hz.compute_width_delta(10, width, 100);
    assert_eq!(
        delta_clamped.milli_points(),
        3_000,
        "Coordinate > 50 must clamp to 50"
    );

    // 2. Optical Kerning with negative max_adjustment (must not panic)
    let hostile_opt_config = OpticalKerningConfig {
        target_gap_area_ratio_permille: 1000,
        max_adjustment: LayoutUnit::from_points(-5),
    };
    let left = vec![(LayoutUnit::from_points(0), LayoutUnit::from_points(10))];
    let right = vec![(LayoutUnit::from_points(0), LayoutUnit::from_points(0))];
    let adj = compute_optical_kerning(
        &left,
        &right,
        LayoutUnit::from_points(20),
        LayoutUnit::from_points(5),
        &hostile_opt_config,
    );
    assert!(adj.milli_points() <= 5_000 && adj.milli_points() >= -5_000);

    // 3. Column Badness Curve edge samples
    let empty_curve = ColumnBadnessCurve {
        column_index: 0,
        min_width: LayoutUnit::from_points(10),
        max_width: LayoutUnit::from_points(100),
        samples: Vec::new(),
    };
    assert_eq!(empty_curve.evaluate(LayoutUnit::from_points(50)), 0);

    let single_sample_curve = ColumnBadnessCurve {
        column_index: 0,
        min_width: LayoutUnit::from_points(10),
        max_width: LayoutUnit::from_points(100),
        samples: vec![(LayoutUnit::from_points(50), 123)],
    };
    assert_eq!(
        single_sample_curve.evaluate(LayoutUnit::from_points(10)),
        123
    );
    assert_eq!(
        single_sample_curve.evaluate(LayoutUnit::from_points(100)),
        123
    );

    // 4. Convex Table Widths with empty, single, and excess budget
    assert_eq!(
        solve_convex_table_widths(&[], LayoutUnit::from_points(100)),
        Vec::<LayoutUnit>::new()
    );
    assert_eq!(
        solve_convex_table_widths(
            std::slice::from_ref(&single_sample_curve),
            LayoutUnit::from_points(150)
        ),
        vec![LayoutUnit::from_points(150)]
    );

    // Table allocator with budget > max_widths sum (excess distributed cleanly)
    let col_a = ColumnBadnessCurve {
        column_index: 0,
        min_width: LayoutUnit::from_points(10),
        max_width: LayoutUnit::from_points(20),
        samples: vec![
            (LayoutUnit::from_points(10), 100),
            (LayoutUnit::from_points(20), 0),
        ],
    };
    let col_b = ColumnBadnessCurve {
        column_index: 1,
        min_width: LayoutUnit::from_points(10),
        max_width: LayoutUnit::from_points(20),
        samples: vec![
            (LayoutUnit::from_points(10), 100),
            (LayoutUnit::from_points(20), 0),
        ],
    };
    let wide_alloc = solve_convex_table_widths(&[col_a, col_b], LayoutUnit::from_points(100));
    assert_eq!(wide_alloc[0] + wide_alloc[1], LayoutUnit::from_points(100));

    // 5. White river detection with negative/zero threshold and depth
    assert!(detect_white_rivers(&[], LayoutUnit::from_points(5), 3).is_empty());
    assert!(detect_white_rivers(&[vec![]], LayoutUnit::from_points(5), 0).is_empty());
    assert!(detect_white_rivers(&[vec![]], LayoutUnit::from_points(-5), 1).is_empty());

    // 6. Baseline Grid Snapping with inverted spring bounds (min > max) and zero pitch
    let inverted_spring = VerticalSpring {
        natural_height: LayoutUnit::from_points(10),
        min_height: LayoutUnit::from_points(20),
        max_height: LayoutUnit::from_points(5), // Inverted
        stiffness: 1,
    };
    let snapped_inverted = snap_blocks_to_baseline_grid(
        &[LayoutUnit::from_points(10)],
        &[inverted_spring],
        LayoutUnit::from_points(0), // Zero grid pitch
    );
    assert_eq!(snapped_inverted.len(), 1);

    // 7. 2D Pagination with oversized paragraph and zero page capacity
    assert!(solve_2d_optimal_pagination(&[], 10, 100, 100).is_empty());
    assert!(
        solve_2d_optimal_pagination(&[ParagraphCandidates { variants: vec![] }], 0, 100, 100)
            .is_empty()
    );

    let oversized_p = ParagraphCandidates {
        variants: vec![ParagraphVariant {
            line_count: 50,
            demerits: 0,
            lines: Vec::new(),
        }],
    };
    // Paragraph has 50 lines, page has 20. First page shouldn't emit break at line 0.
    let oversized_breaks = solve_2d_optimal_pagination(&[oversized_p], 20, 1000, 1000);
    assert_eq!(
        oversized_breaks.len(),
        0,
        "Single paragraph on fresh page does not create leading empty page break"
    );
}
