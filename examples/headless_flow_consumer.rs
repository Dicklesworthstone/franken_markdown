//! Tiny FrankenMarkdown-owned headless flow consumer (FCB-074.B).
//!
//! Plan §12.9 & §27.4:
//! "Add a tiny FrankenMarkdown-owned headless flow consumer before the FCB
//!  integration passes. It measures/layouts the same document, validates nested
//!  provenance and budgets, and serializes a deterministic semantic layout
//!  fixture with no FCB dependency."

#![forbid(unsafe_code)]

use std::env;
use std::fs;
use std::io::{self, Read};

use franken_markdown::{
    FlowBudgets, FlowConstraints, HeadlessFlowConsumer, ProvenanceOracle, ResumableFlowDisplay,
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = env::args().collect();
    let source = if let Some(path) = args.get(1) {
        fs::read_to_string(path)?
    } else {
        let mut buffer = String::new();
        let _ = io::stdin().read_to_string(&mut buffer)?;
        if buffer.is_empty() {
            // Default self-contained demonstration document
            "# FrankenMarkdown Flow Consumer\n\n\
             This headless consumer measures continuous flow, checks provenance, \
             and serializes deterministic fixtures without FCB or GPU dependencies.\n\n\
             ## Features\n\n\
             - Continuous flow paragraph layout\n\
             - Budget defense against hostile inputs\n\
             - Provenance oracle truthfulness\n\
             - Renderer-neutral display list emission\n\n\
             ![Architecture Diagram](assets/flow_arch.png)\n"
                .to_string()
        } else {
            buffer
        }
    };

    println!("=== 1. CONTINUOUS-FLOW LAYOUT ===");
    let constraints = FlowConstraints {
        viewport_width: 80,
        line_height: 16,
        char_width: 1,
        max_viewport_lines: None,
    };
    let budgets = FlowBudgets::default();
    let consumer = HeadlessFlowConsumer::new(constraints, budgets);
    let output = consumer.consume_source(&source)?;

    println!("Lines laid out: {}", output.lines.len());
    println!("Total height: {} units", output.total_height);
    println!("Total width: {} units", output.total_width);
    println!("Consumed blocks: {}", output.consumed_blocks);
    println!("Consumed bytes: {}", output.consumed_bytes);

    println!("\n=== 2. PROVENANCE ORACLE VERIFICATION ===");
    let report = ProvenanceOracle::verify_truthfulness(
        output.source_map.provenance_graph(),
        &source,
        &|_| None,
    )?;
    println!("Total provenance nodes: {}", report.total_nodes);
    println!(
        "Zero invented contiguous slices: {}",
        report.zero_invented_contiguous_slices
    );
    assert!(
        report.zero_invented_contiguous_slices,
        "provenance must never invent contiguous slices"
    );

    println!("\n=== 3. SYNCHRONOUS-RESUMABLE FLOW DISPLAY ===");
    let mut resumable = ResumableFlowDisplay::new(&source, 2);
    let mut step_count = 0;
    while let Some(step) = resumable.step()? {
        step_count += 1;
        println!(
            "Step {}: produced {} blocks, {} unresolved assets (total {})",
            step_count,
            step.blocks.len(),
            step.unresolved_assets.len(),
            step.total_blocks
        );
    }
    let display_list = resumable.to_display_list();
    println!(
        "Materialized DisplayList: {} items, {} reading nodes",
        display_list.items().len(),
        display_list.reading_order().len()
    );

    println!("\n=== 4. DETERMINISTIC SEMANTIC FIXTURE ===");
    let fixture = output.to_semantic_fixture();
    print!("{fixture}");

    Ok(())
}
