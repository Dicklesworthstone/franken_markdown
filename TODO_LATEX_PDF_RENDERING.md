# LaTeX-Grade PDF Rendering TODO

Status: active implementation tracker  
Owner: multi-agent (`PeachCove` coordinating layout/TODO/beads; `DustyCliff` currently drafting `src/pdf.rs` embedded-font writer)  
Last updated: 2026-06-27

This tracker expands the PDF-quality slate into concrete work. Beads are the
source of truth for status; this file keeps the execution order and acceptance
details easy to scan while implementation is moving quickly.

## Execution Order

1. **Do not overwrite `src/pdf.rs` peer work.**
   - Current state: `DustyCliff` has an uncommitted CIDFontType2/Identity-H
     draft in `src/pdf.rs`.
   - `PeachCove` must not edit `src/pdf.rs` until that draft is pushed or handed
     off through Agent Mail.

2. **Finish font/text foundations.**
   - Broad bead: `br-best-in-class-markdown-renderer-fmd-agent-ergonomics-commonma-vxi`
   - New children:
     - `br-best-in-class-markdown-renderer-fmd-agent-ergonomics-commonma-vxi.1`
       PDF: deterministic fixed-point text metrics (**done**)
     - `br-best-in-class-markdown-renderer-fmd-agent-ergonomics-commonma-vxi.2`
       Fonts: parse kern table and apply pair kerning (**done**)
     - `br-best-in-class-markdown-renderer-fmd-agent-ergonomics-commonma-vxi.3`
       Fonts: implement focused GSUB ligature shaping
     - `br-best-in-class-markdown-renderer-fmd-agent-ergonomics-commonma-vxi.4`
       Fonts: implement focused GPOS pair positioning
   - Acceptance:
     - all metric math has a deterministic fixed-point representation,
     - font-backed measurement replaces approximate PDF widths,
     - kerning and ligatures are optional/focused but test-covered,
     - browser/WASM builds remain free of filesystem/system-font assumptions.

3. **Preserve styled PDF inline runs.**
   - Bead: `br-best-in-class-markdown-renderer-fmd-agent-ergonomics-commonma-dy5.1`
   - Replace string-flattening before PDF layout with styled run metadata:
     body/bold/italic/code/link/color/source span.
   - Acceptance:
     - bold, italic, code, and links survive into layout primitives,
     - plain-text extraction remains available for fallback,
     - tests prove style boundaries are preserved.

4. **Implement TeX paragraph primitives.**
   - Bead: `br-best-in-class-markdown-renderer-fmd-agent-ergonomics-commonma-dy5.2` (**done**)
   - Add box/glue/penalty paragraph IR.
   - Acceptance:
     - explicit `Box`, `Glue`, `Penalty` model,
     - deterministic width/stretch/shrink units,
     - conversion from simple text/runs into paragraph items,
     - no dependency or `unsafe` additions.

5. **Implement Knuth-Plass line breaking.**
   - Bead: `br-best-in-class-markdown-renderer-fmd-agent-ergonomics-commonma-dy5.3` (**first optimizer done**)
   - Depends on `dy5.2` and `vxi.1`.
   - Acceptance:
     - paragraph-wide active breakpoint optimization,
     - badness/demerits/fitness classes,
     - flagged penalty and consecutive-hyphen hooks,
     - emergency fallback for impossible paragraphs,
     - fixtures where optimal breaks differ from greedy wrapping.

6. **Add Liang/TeX hyphenation.**
   - Bead: `br-best-in-class-markdown-renderer-fmd-agent-ergonomics-commonma-dy5.4` (**starter core done**)
   - Follow-up corpus bead: `br-best-in-class-markdown-renderer-fmd-agent-ergonomics-commonma-dy5.6`
   - Acceptance:
     - compact deterministic pattern trie or equivalent table,
     - English patterns initially,
     - left/right minima and exceptions,
     - discretionary hyphen penalties feed the line breaker.

7. **Add microtypography hooks.**
   - Bead: `br-best-in-class-markdown-renderer-fmd-agent-ergonomics-commonma-dy5.5` (**done**)
   - Acceptance:
     - punctuation protrusion tables,
     - optional font expansion budget,
     - optical margin alignment,
     - disabled-by-default or policy-controlled deterministic behavior,
     - visual gauntlet fixtures.

8. **Implement TeX-like vertical page builder.**
   - Bead: `br-best-in-class-markdown-renderer-fmd-agent-ergonomics-commonma-49d.1`
   - Acceptance:
     - vertical boxes/glue/penalties,
     - best page-break cost selection,
     - widow/orphan/club penalties,
     - heading keep-with-next,
     - baseline-grid/leading hooks.

9. **Implement high-quality block layout.**
   - Broad bead: `br-best-in-class-markdown-renderer-fmd-agent-ergonomics-commonma-49d`
   - New children:
     - `br-best-in-class-markdown-renderer-fmd-agent-ergonomics-commonma-49d.2`
       booktabs-quality table layout
     - `br-best-in-class-markdown-renderer-fmd-agent-ergonomics-commonma-49d.3`
       syntax-highlighted code block layout
     - `br-best-in-class-markdown-renderer-fmd-agent-ergonomics-commonma-49d.4`
       polished blockquote/list/heading layout rules
   - Acceptance:
     - tables have measured columns, repeated headers, subtle striping, and sane
       page breaks,
     - code blocks wrap/page-break instead of clipping,
     - lists use hanging markers and aligned ordered counters,
     - blockquotes never strand visual bars or padding.

10. **Finish compact PDF writer features.**
    - Broad bead: `br-best-in-class-markdown-renderer-fmd-agent-ergonomics-commonma-fep`
    - New children:
      - `br-best-in-class-markdown-renderer-fmd-agent-ergonomics-commonma-fep.2`
        Type0 CIDFontType2 subset embedding with ToUnicode (**landed in `91d4707`; bead close currently blocked by broad parent state**)
      - `br-best-in-class-markdown-renderer-fmd-agent-ergonomics-commonma-fep.3`
        links, outlines, metadata, and annotations
      - `br-best-in-class-markdown-renderer-fmd-agent-ergonomics-commonma-fep.4`
        tagged PDF accessibility structure
      - `br-best-in-class-markdown-renderer-fmd-agent-ergonomics-commonma-fep.5`
        stream compression and size optimization
    - Acceptance:
      - embedded subset fonts replace Base-14 output,
      - Unicode text copies/searches correctly,
      - PDF annotations/outlines are deterministic,
      - streams compress deterministically,
      - tagged structure exists for headings/lists/tables/links/images/code.

11. **Build quality/performance evidence.**
    - Broad bead: `br-best-in-class-markdown-renderer-fmd-agent-ergonomics-commonma-qw1`
    - New child:
      - `br-best-in-class-markdown-renderer-fmd-agent-ergonomics-commonma-qw1.1`
        TeX-style linebreak and PDF visual gauntlet
    - Acceptance:
      - line-break fixture suite,
      - badness/raggedness/hyphen-count ledger,
      - PDF structural validation,
      - raster/visual comparisons,
      - file-size ledger,
      - deterministic perf baselines under `tests/artifacts/`.

## Immediate Implementation Focus

- [x] Create granular beads for the LaTeX/PDF slate.
- [x] Add dependencies and verify the bead graph has no cycles.
- [x] Notify `DustyCliff` about the work split and `src/pdf.rs` conflict.
- [x] Implement `vxi.1` fixed-point text metrics in `src/layout.rs`.
- [x] Add unit tests in `tests/layout_test.rs`.
- [x] Run targeted layout checks.
- [x] Implement `vxi.2` legacy `kern` format-0 pair kerning in `src/text.rs`.
- [x] Wire pair kerning into fixed-point layout measurement.
- [x] Implement `dy5.2` box/glue/penalty primitives in `src/layout.rs`.
- [x] Implement `dy5.3` first Knuth-Plass active breakpoint optimizer.
- [x] Implement `dy5.4` Liang-style hyphenation core with starter English corpus.
- [ ] Expand to full TeX English hyphenation corpus (`dy5.6`).
- [x] Implement `dy5.5` deterministic microtypography hooks.
- [ ] Sync beads and commit only owned files.
- [ ] Re-check Agent Mail before touching `src/pdf.rs`.

## Implementation Notes

- The render core must remain dependency-free, synchronous, deterministic, and
  `wasm32-unknown-unknown` compatible.
- Asupersync belongs in native orchestration, not inside paragraph shaping or
  PDF layout primitives.
- Use fixed-point integer units for layout decisions. `f32` may remain at the
  final PDF serialization boundary if needed, but it should not decide line
  breaks.
- Every advanced feature needs a fallback path: if shaping, hyphenation,
  microtypography, or page optimization cannot produce a valid result, the
  renderer should degrade to deterministic simpler layout rather than failing
  or panicking.
