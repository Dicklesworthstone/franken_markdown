#![forbid(unsafe_code)]

//! Integration tests for code fence flow and constrained table layout (FCB-033.A).
//!
//! Acceptance criteria & oracle contract:
//! - Exact code copy: Authoritative raw code bytes are preserved intact, without visual
//!   line numbers or gutter decorators.
//! - Virtualized line windowing: Giant code fences (e.g. 5,000 lines) only materialize
//!   visible lines within the viewport, without whole-fence clones.
//! - Independent horizontal scrolling: Code fences and wide tables scroll horizontally
//!   via `scroll_x` rather than causing unexpected global document width changes.
//! - Bounded column measurement: Tables compute column constraints enforcing minimum
//!   column width to avoid text crushing, and maximum column width to bound expansion.
//! - Late wide cell handling: An unmeasured row with a wide cell expands the column up
//!   to `max_col_width` cleanly.
//! - Row virtualization: Large tables (e.g. 1,000 rows) virtualize body rows while
//!   preserving headers and accessibility structure.
//! - Negative controls: Exceeding column budget (>64 columns) or invalid indices are refused.

use franken_markdown::ast::Align;
use franken_markdown::code_table_flow::{
    CodeFenceFlow, CodeTableError, ConstrainedTableFlow, TableCell, TableConstraints,
    MAX_COLUMNS_BUDGET,
};
use franken_markdown::display::{DisplayItem, DisplayRect};
use franken_markdown::span::SourceSpan;

#[test]
fn oracle_exact_code_copy_preserves_verbatim_bytes() {
    let source = "def process_batch(items: list[str]) -> int:\n    # Indented block\n    total = 0\n    for item in items:\n        total += len(item)\n    return total\n";
    let span = SourceSpan::new(100, 100 + source.len());
    let flow = CodeFenceFlow::new(Some("python".to_string()), source.to_string(), span);

    // Exact copy invariant: returns byte-for-byte identical code
    assert_eq!(flow.exact_code_copy(), source);
    assert_eq!(flow.lang(), Some("python"));
    assert_eq!(flow.source_span(), span);
    assert_eq!(flow.line_count(), 6);
}

#[test]
fn oracle_giant_fence_line_windowing_no_whole_fence_clone() {
    // 5,000 lines of code
    let mut code = String::new();
    for i in 0..5000 {
        code.push_str(&format!("const DATA_ENTRY_{i:04}: u64 = 0x{i:08X};\n"));
    }

    let flow = CodeFenceFlow::new(Some("rust".to_string()), code, SourceSpan::default());
    assert_eq!(flow.line_count(), 5000);

    let container_bounds = DisplayRect::new(0.0, 0.0, 800.0, flow.total_height());

    // Viewport window: 300pt height at offset 2,000pt
    let dl = flow
        .materialize_viewport(container_bounds, 2000.0, 300.0)
        .expect("materialize line window");

    // Count text runs emitted
    let rendered_lines = dl
        .items()
        .iter()
        .filter(|item| matches!(item, DisplayItem::Text(_)))
        .count();

    // Line height is ~18.85pt, so 300pt viewport contains ~16 lines (+1 margin = ~17 lines)
    assert!(rendered_lines > 0);
    assert!(
        rendered_lines <= 22,
        "only visible window must be emitted for 5,000-line fence, got {rendered_lines}"
    );

    // Reading tree still retains full semantic structure
    let reading_nodes = dl.reading_order();
    assert_eq!(reading_nodes.len(), 1);
}

#[test]
fn oracle_code_fence_independent_horizontal_scrolling() {
    let long_line = "let very_long_identifier_name_exceeding_standard_line_length_to_test_horizontal_scrolling = compute();\n";
    let mut flow = CodeFenceFlow::new(
        Some("rust".to_string()),
        long_line.to_string(),
        SourceSpan::default(),
    );

    let bounds = DisplayRect::new(0.0, 0.0, 300.0, flow.total_height());

    // Scroll right by 50pt
    flow.scroll_x = 50.0;

    let dl = flow
        .materialize_viewport(bounds, 0.0, 200.0)
        .expect("materialize scrolled code fence");

    // Verify text run position reflects scroll_x offset
    let text_run = dl
        .items()
        .iter()
        .find_map(|item| match item {
            DisplayItem::Text(t) => Some(t),
            _ => None,
        })
        .expect("must contain text run");

    // At bounds.x = 0, padding_left = 12, scroll_x = 50 -> x = 12 - 50 = -38.0
    assert!((text_run.bounds.x - (-38.0)).abs() < 1e-3);
}

#[test]
fn oracle_wide_table_scrolls_horizontally_without_crushing_columns() {
    // 6 columns in a 300pt wide container.
    // Minimum column width of 70pt means minimum table width is 420pt > 300pt.
    let headers = vec![
        TableCell::new("Timestamp", SourceSpan::default()),
        TableCell::new("Severity", SourceSpan::default()),
        TableCell::new("Component", SourceSpan::default()),
        TableCell::new("Operation", SourceSpan::default()),
        TableCell::new("Status", SourceSpan::default()),
        TableCell::new("Latency", SourceSpan::default()),
    ];
    let alignments = vec![Align::Left; 6];
    let rows = vec![vec![
        TableCell::new("2026-09-16T12:00:00Z", SourceSpan::default()),
        TableCell::new("INFO", SourceSpan::default()),
        TableCell::new("Renderer", SourceSpan::default()),
        TableCell::new("MaterializeViewport", SourceSpan::default()),
        TableCell::new("Success", SourceSpan::default()),
        TableCell::new("1.2ms", SourceSpan::default()),
    ]];

    let constraints = TableConstraints {
        min_col_width: 70.0,
        max_col_width: 300.0,
        container_width: 300.0, // Narrow container
    };

    let mut table = ConstrainedTableFlow::try_new(
        alignments,
        headers,
        rows,
        SourceSpan::default(),
        constraints,
    )
    .expect("build wide table");

    // Table width exceeds container width
    assert!(table.total_table_width() > 300.0);
    assert!(table.requires_horizontal_scroll());

    // Scroll horizontally by 40pt
    table.scroll_x = 40.0;

    let dl = table
        .materialize_viewport(0.0, 0.0, 0.0, 400.0)
        .expect("materialize scrolled table");

    // First header cell starts at origin_x - scroll_x = -40.0
    let first_header = dl
        .items()
        .iter()
        .find_map(|item| match item {
            DisplayItem::Text(t) if t.color_role == "table-header" => Some(t),
            _ => None,
        })
        .expect("first header");

    assert!((first_header.bounds.x - (-40.0)).abs() < 1e-3);
}

#[test]
fn oracle_late_wide_cell_in_unmeasured_row() {
    let headers = vec![TableCell::new("Short", SourceSpan::default())];
    let alignments = vec![Align::Left];
    let rows = vec![vec![TableCell::new("A", SourceSpan::default())]];

    let constraints = TableConstraints {
        min_col_width: 50.0,
        max_col_width: 180.0,
        container_width: 800.0,
    };

    let mut table = ConstrainedTableFlow::try_new(
        alignments,
        headers,
        rows,
        SourceSpan::default(),
        constraints,
    )
    .unwrap();

    let initial_col_w = table.column_width(0).unwrap();

    // Late wide cell in row 500
    let late_cell = "This is a late discovery of a very wide data entry in row 500";
    let expanded = table.handle_late_wide_cell(0, late_cell).unwrap();
    assert!(expanded);

    let refined_w = table.column_width(0).unwrap();
    assert!(refined_w > initial_col_w);
    assert!(
        refined_w <= 180.0,
        "must be bounded by max_col_width (180.0), got {refined_w}"
    );
}

#[test]
fn oracle_large_table_row_virtualization() {
    let headers = vec![
        TableCell::new("ColA", SourceSpan::default()),
        TableCell::new("ColB", SourceSpan::default()),
    ];
    let alignments = vec![Align::Left, Align::Left];

    // 1,500 rows
    let mut rows = Vec::new();
    for i in 0..1500 {
        rows.push(vec![
            TableCell::new(format!("Key_{i}"), SourceSpan::default()),
            TableCell::new(format!("Value_{i}"), SourceSpan::default()),
        ]);
    }

    let table = ConstrainedTableFlow::try_new(
        alignments,
        headers,
        rows,
        SourceSpan::default(),
        TableConstraints::default(),
    )
    .unwrap();

    assert_eq!(table.row_count(), 1500);

    // Viewport is 200pt tall at vertical offset 600pt
    let dl = table
        .materialize_viewport(0.0, 0.0, 600.0, 200.0)
        .expect("materialize table");

    let cell_count = dl
        .items()
        .iter()
        .filter(|item| match item {
            DisplayItem::Text(t) => t.color_role == "table-cell",
            _ => false,
        })
        .count();

    // Each row has 2 cells; in 200pt (row height 28pt) there are ~8 rows = ~16 cells
    assert!(cell_count > 0);
    assert!(
        cell_count <= 30,
        "only visible rows must be emitted, got {cell_count}"
    );
}

#[test]
fn oracle_negative_controls_excessive_columns_and_out_of_bounds() {
    // Negative control 1: column count exceeding MAX_COLUMNS_BUDGET (64)
    let too_many_headers = vec![TableCell::new("X", SourceSpan::default()); MAX_COLUMNS_BUDGET + 1];
    let alignments = vec![Align::Left; MAX_COLUMNS_BUDGET + 1];

    let err = ConstrainedTableFlow::try_new(
        alignments,
        too_many_headers,
        Vec::new(),
        SourceSpan::default(),
        TableConstraints::default(),
    );

    assert!(matches!(
        err,
        Err(CodeTableError::ColumnBudgetExceeded { .. })
    ));

    // Negative control 2: invalid column index query
    let valid_table = ConstrainedTableFlow::try_new(
        vec![Align::Left],
        vec![TableCell::new("Header", SourceSpan::default())],
        Vec::new(),
        SourceSpan::default(),
        TableConstraints::default(),
    )
    .unwrap();

    let col_err = valid_table.column_width(10);
    assert!(matches!(
        col_err,
        Err(CodeTableError::InvalidColumnIndex { .. })
    ));
}
