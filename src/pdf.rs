//! PDF renderer with embedded subset fonts.
//!
//! Produces a deterministic, **tiny** PDF that embeds document-specific *subsets*
//! of the bundled IBM Plex / Computer Modern faces (see [`crate::fonts`] +
//! [`crate::text::Font::subset`]). Text is laid out with the faces' real `hmtx`
//! metrics, then written as a composite `Type0` font with `Identity-H` encoding
//! (2-byte glyph ids) and a `CIDFontType2` descendant carrying the subset
//! `FontFile2`. Each face also gets a `ToUnicode` CMap so the text stays
//! selectable / copy-pasteable.
//!
//! Greedy line wrapping + automatic pagination over headings, paragraphs, code
//! blocks, lists, blockquotes, tables (simple), and rules. Focused GPOS kerning
//! and GSUB ligatures are applied today; Knuth-Plass optimal breaking, richer
//! page layout, and FlateDecode compression are the next increments.
//!
//! Pure computation (no `std::fs`, no deps) so it stays WASM / `--no-default-features`
//! clean; the font bytes come from `include_bytes!`, not the filesystem.

use crate::PdfOptions;
use crate::ast::{Block, Document, Inline, List};
use crate::error::Result;
use crate::fonts::{self, FontStyle};
use crate::text::{Font, Kerning, Ligatures};
use std::collections::{BTreeMap, BTreeSet};

const PAGE_W: f32 = 612.0; // US Letter, points
const PAGE_H: f32 = 792.0;
const MARGIN: f32 = 72.0;
const CONTENT_W: f32 = PAGE_W - 2.0 * MARGIN;

// Font slots referenced in page Resources as /F1../F4.
const F_BODY: u8 = 1;
const F_BOLD: u8 = 2;
const F_ITALIC: u8 = 3;
const F_MONO: u8 = 4;
const SLOTS: [u8; 4] = [F_BODY, F_BOLD, F_ITALIC, F_MONO];

/// One laid-out, pre-wrapped line of text positioned by the paginator.
struct Line {
    x: f32,
    size: f32,
    font: u8,
    text: String,
    gap_after: f32,
    rule: bool,
}

/// The four source faces resolved from the theme family + the registry.
struct Faces {
    body: Font,
    bold: Font,
    italic: Font,
    mono: Font,
}

impl Faces {
    fn load(opts: &PdfOptions) -> Option<Self> {
        let fam = opts.theme.font;
        Some(Self {
            body: fonts::load_body(fam, FontStyle::Regular).ok()?,
            bold: fonts::load_body(fam, FontStyle::Bold).ok()?,
            italic: fonts::load_body(fam, FontStyle::Italic).ok()?,
            mono: fonts::load_mono(FontStyle::Regular).ok()?,
        })
    }

    fn get(&self, slot: u8) -> &Font {
        match slot {
            F_BOLD => &self.bold,
            F_ITALIC => &self.italic,
            F_MONO => &self.mono,
            _ => &self.body,
        }
    }

    /// Advance of `c` in 1/1000 em (PDF text space) for the slot's face.
    fn advance(&self, slot: u8, c: char) -> f32 {
        self.get(slot).advance_1000(c) as f32
    }
}

/// Render a document to PDF bytes.
///
/// # Errors
/// Infallible in practice (the bundled fonts always parse); returns [`Result`]
/// to leave room for future validation without a signature change.
pub fn render(doc: &Document, opts: &PdfOptions) -> Result<Vec<u8>> {
    let Some(faces) = Faces::load(opts) else {
        // The bundled fonts are tested to parse, so this is unreachable in
        // practice; emit a valid empty document rather than failing.
        return Ok(empty_pdf());
    };
    let lines = layout(&doc.blocks, opts, &faces);
    Ok(serialize(&lines, opts, &faces))
}

// ---- layout -----------------------------------------------------------------

fn layout(blocks: &[Block], opts: &PdfOptions, faces: &Faces) -> Vec<Line> {
    let mut out = Vec::new();
    layout_blocks(blocks, 0.0, &mut out, opts, faces);
    out
}

fn layout_blocks(
    blocks: &[Block],
    indent: f32,
    out: &mut Vec<Line>,
    opts: &PdfOptions,
    faces: &Faces,
) {
    for block in blocks {
        layout_block(block, indent, out, opts, faces);
    }
}

fn layout_block(block: &Block, indent: f32, out: &mut Vec<Line>, opts: &PdfOptions, faces: &Faces) {
    match block {
        Block::Heading { level, inlines } => {
            let size = match level {
                1 => 24.0,
                2 => 19.0,
                3 => 16.0,
                4 => 13.5,
                5 => 12.0,
                _ => 11.0,
            };
            push_wrapped(&inline_text(inlines), indent, size, F_BOLD, 6.0, out, faces);
        }
        Block::Paragraph(inlines) => {
            push_wrapped(&inline_text(inlines), indent, 11.0, F_BODY, 7.0, out, faces);
        }
        Block::CodeBlock { code, .. } => {
            for raw in code.lines() {
                let clipped = clip_to_width(raw, CONTENT_W - indent - 8.0, 9.5, F_MONO, faces);
                out.push(Line {
                    x: MARGIN + indent + 8.0,
                    size: 9.5,
                    font: F_MONO,
                    text: clipped,
                    gap_after: 1.5,
                    rule: false,
                });
            }
            gap(out, 6.0);
        }
        Block::BlockQuote(inner) => {
            layout_blocks(inner, indent + 18.0, out, opts, faces);
            gap(out, 3.0);
        }
        Block::List(list) => layout_list(list, indent, out, opts, faces),
        Block::Table(table) => {
            let header = table
                .head
                .iter()
                .map(|c| inline_text(c))
                .collect::<Vec<_>>();
            push_wrapped(
                &header.join("   |   "),
                indent,
                11.0,
                F_BOLD,
                2.0,
                out,
                faces,
            );
            for row in &table.rows {
                let cells = row.iter().map(|c| inline_text(c)).collect::<Vec<_>>();
                push_wrapped(
                    &cells.join("   |   "),
                    indent,
                    11.0,
                    F_BODY,
                    2.0,
                    out,
                    faces,
                );
            }
            gap(out, 6.0);
        }
        Block::ThematicBreak => {
            out.push(Line {
                x: MARGIN + indent,
                size: 6.0,
                font: F_BODY,
                text: String::new(),
                gap_after: 8.0,
                rule: true,
            });
        }
        Block::HtmlBlock(html) => {
            if !opts.allow_raw_html {
                push_wrapped(html, indent, 11.0, F_BODY, 7.0, out, faces);
            }
        }
    }
}

fn layout_list(list: &List, indent: f32, out: &mut Vec<Line>, _opts: &PdfOptions, faces: &Faces) {
    for (i, item) in list.items.iter().enumerate() {
        let marker = match item.task {
            Some(true) => "[x]".to_string(),
            Some(false) => "[ ]".to_string(),
            None if list.ordered => format!("{}.", list.start + i as u64),
            None => "•".to_string(),
        };
        let body = item
            .blocks
            .iter()
            .map(|b| match b {
                Block::Paragraph(inl) => inline_text(inl),
                other => block_plain(other),
            })
            .collect::<Vec<_>>()
            .join(" ");
        let text = format!("{marker}  {body}");
        push_wrapped(&text, indent + 16.0, 11.0, F_BODY, 2.0, out, faces);
    }
    gap(out, 6.0);
}

fn push_wrapped(
    text: &str,
    indent: f32,
    size: f32,
    font: u8,
    gap_after: f32,
    out: &mut Vec<Line>,
    faces: &Faces,
) {
    let max = (CONTENT_W - indent).max(40.0);
    let wrapped = wrap(text, max, size, font, faces);
    let n = wrapped.len();
    if n == 0 {
        gap(out, gap_after);
        return;
    }
    for (idx, l) in wrapped.into_iter().enumerate() {
        out.push(Line {
            x: MARGIN + indent,
            size,
            font,
            text: l,
            gap_after: if idx + 1 == n { gap_after } else { 0.0 },
            rule: false,
        });
    }
}

fn gap(out: &mut [Line], amount: f32) {
    if let Some(last) = out.last_mut() {
        last.gap_after += amount;
    }
}

// ---- pagination + serialization --------------------------------------------

fn serialize(lines: &[Line], opts: &PdfOptions, faces: &Faces) -> Vec<u8> {
    // Which slots actually appear (skip embedding unused faces).
    let mut used_slots: Vec<u8> = SLOTS
        .into_iter()
        .filter(|&s| {
            lines
                .iter()
                .any(|l| !l.rule && !l.text.is_empty() && l.font == s)
        })
        .collect();
    if used_slots.is_empty() {
        used_slots.push(F_BODY); // always embed at least one face
    }

    // Subset each used face to the characters it renders, and keep the parsed
    // subset (its cmap gives the new glyph ids we encode in the content stream).
    let mut subsets: Vec<EmbeddedFace> = Vec::new();
    for &slot in &used_slots {
        let source = faces.get(slot);
        let lig = source.gsub_ligatures();
        let mut chars: BTreeSet<char> = BTreeSet::new();
        let mut shaped_glyphs: BTreeSet<u16> = BTreeSet::new();
        let mut lig_src_uni: BTreeMap<u16, String> = BTreeMap::new();
        for l in lines.iter().filter(|l| !l.rule && l.font == slot) {
            chars.extend(l.text.chars());
            let (shaped, ligs) = shape(source, &lig, &l.text);
            shaped_glyphs.extend(shaped);
            for (g, s) in ligs {
                lig_src_uni.entry(g).or_insert(s);
            }
        }
        let keep: Vec<char> = chars.into_iter().collect();
        // Seed the subset with the chars' glyphs (so the cmap resolves) plus the
        // shaped glyphs (which add ligature glyphs no character maps to).
        let mut seed: Vec<u16> = keep.iter().map(|&c| source.glyph_index(c)).collect();
        seed.extend(shaped_glyphs);
        let Some((bytes, map)) = source.subset_glyphs(&seed, &keep) else {
            return empty_pdf();
        };
        let Ok(font) = Font::parse(bytes.clone()) else {
            return empty_pdf();
        };
        // Re-key ligature ToUnicode entries by the new (subset) glyph id.
        let mut lig_uni: BTreeMap<u16, String> = BTreeMap::new();
        for (src, s) in lig_src_uni {
            if let Some(&new) = map.get(&src) {
                lig_uni.insert(new, s);
            }
        }
        subsets.push(EmbeddedFace {
            slot,
            bytes,
            font,
            kern: source.gpos_kerning(),
            lig,
            map,
            lig_uni,
        });
    }

    // Build per-page content streams, encoding text as Identity-H glyph ids.
    let mut pages: Vec<String> = Vec::new();
    let mut content = String::new();
    let mut y = PAGE_H - MARGIN;
    let top = PAGE_H - MARGIN;
    for line in lines {
        let leading = line.size * 1.32;
        if y - leading < MARGIN {
            pages.push(std::mem::take(&mut content));
            y = top;
        }
        y -= leading;
        if line.rule {
            let x2 = PAGE_W - MARGIN;
            content.push_str(&format!(
                "0.82 0.82 0.84 RG 0.7 w {x:.2} {yy:.2} m {x2:.2} {yy:.2} l S\n",
                x = line.x,
                yy = y + line.size * 0.5,
            ));
        } else if !line.text.is_empty() {
            if let Some(face) = subsets.iter().find(|f| f.slot == line.font) {
                let source = faces.get(line.font);
                let gids: Vec<u16> = line.text.chars().map(|c| source.glyph_index(c)).collect();
                let shaped = face.lig.substitute(&gids);
                content.push_str(&format!(
                    "BT /F{f} {s:.2} Tf 1 0 0 1 {x:.2} {y:.2} Tm {tj} TJ ET\n",
                    f = line.font,
                    s = line.size,
                    x = line.x,
                    y = y,
                    tj = kerned_tj(&face.map, source, &face.kern, &shaped),
                ));
            }
        }
        y -= line.gap_after;
    }
    pages.push(content);
    if pages.iter().all(String::is_empty) {
        pages = vec![String::new()];
    }

    build_pdf(&pages, &subsets, opts)
}

/// A subset face ready to embed.
struct EmbeddedFace {
    slot: u8,
    bytes: Vec<u8>,
    font: Font,
    /// GPOS pair kerning of the SOURCE face (the subset drops GPOS), keyed by
    /// original glyph ids — used to position glyphs in the content stream.
    kern: Kerning,
    /// GSUB ligatures of the SOURCE face, applied to shape content lines.
    lig: Ligatures,
    /// Source glyph id -> subset (renumbered) glyph id.
    map: BTreeMap<u16, u16>,
    /// Subset glyph id -> its source characters, for ligature glyphs that no
    /// character maps to (keeps ligated text selectable via ToUnicode).
    lig_uni: BTreeMap<u16, String>,
}

fn build_pdf(pages: &[String], faces: &[EmbeddedFace], opts: &PdfOptions) -> Vec<u8> {
    let p = pages.len();
    let nf = faces.len();
    let title = opts.title.clone().unwrap_or_default();

    // Object number plan (1-indexed):
    //   1 Catalog, 2 Pages, [3..3+p) Page objs, [3+p..3+2p) content streams,
    //   then per face k: type0, cidfont, descriptor, fontfile, tounicode (5),
    //   then Info (optional).
    let page_obj = |i: usize| 3 + i;
    let content_obj = |i: usize| 3 + p + i;
    let face_base = 3 + 2 * p;
    let type0_obj = |k: usize| face_base + 5 * k;
    let cid_obj = |k: usize| face_base + 5 * k + 1;
    let desc_obj = |k: usize| face_base + 5 * k + 2;
    let file_obj = |k: usize| face_base + 5 * k + 3;
    let touni_obj = |k: usize| face_base + 5 * k + 4;
    let info_obj = face_base + 5 * nf;
    let total_objs = if title.is_empty() {
        info_obj - 1
    } else {
        info_obj
    };

    let mut buf: Vec<u8> = Vec::new();
    let mut offsets: Vec<usize> = vec![0; total_objs + 1];

    let emit = |buf: &mut Vec<u8>, offsets: &mut Vec<usize>, n: usize, body: &str| {
        offsets[n] = buf.len();
        buf.extend_from_slice(format!("{n} 0 obj\n{body}\nendobj\n").as_bytes());
    };

    buf.extend_from_slice(b"%PDF-1.7\n%\xE2\xE3\xCF\xD3\n");

    emit(
        &mut buf,
        &mut offsets,
        1,
        "<< /Type /Catalog /Pages 2 0 R >>",
    );

    let kids = (0..p)
        .map(|i| format!("{} 0 R", page_obj(i)))
        .collect::<Vec<_>>()
        .join(" ");
    emit(
        &mut buf,
        &mut offsets,
        2,
        &format!("<< /Type /Pages /Count {p} /Kids [ {kids} ] >>"),
    );

    // Shared font resource dict referencing every embedded face's Type0 object.
    let font_res = faces
        .iter()
        .enumerate()
        .map(|(k, f)| format!("/F{} {} 0 R", f.slot, type0_obj(k)))
        .collect::<Vec<_>>()
        .join(" ");
    for i in 0..p {
        emit(
            &mut buf,
            &mut offsets,
            page_obj(i),
            &format!(
                "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 {PAGE_W:.0} {PAGE_H:.0}] \
                 /Resources << /Font << {font_res} >> >> /Contents {c} 0 R >>",
                c = content_obj(i),
            ),
        );
    }

    for (i, page) in pages.iter().enumerate() {
        offsets[content_obj(i)] = buf.len();
        buf.extend_from_slice(
            format!(
                "{n} 0 obj\n<< /Length {len} >>\nstream\n{page}endstream\nendobj\n",
                n = content_obj(i),
                len = page.len(),
            )
            .as_bytes(),
        );
    }

    // Embedded font object groups.
    for (k, face) in faces.iter().enumerate() {
        let psname = subset_psname(k, face.slot);
        let m = FaceMetrics::of(&face.font);

        emit(
            &mut buf,
            &mut offsets,
            type0_obj(k),
            &format!(
                "<< /Type /Font /Subtype /Type0 /BaseFont /{psname} /Encoding /Identity-H \
                 /DescendantFonts [{cid} 0 R] /ToUnicode {tu} 0 R >>",
                cid = cid_obj(k),
                tu = touni_obj(k),
            ),
        );
        emit(
            &mut buf,
            &mut offsets,
            cid_obj(k),
            &format!(
                "<< /Type /Font /Subtype /CIDFontType2 /BaseFont /{psname} \
                 /CIDSystemInfo << /Registry (Adobe) /Ordering (Identity) /Supplement 0 >> \
                 /FontDescriptor {desc} 0 R /CIDToGIDMap /Identity /DW 1000 /W [{w}] >>",
                desc = desc_obj(k),
                w = widths_array(&face.font),
            ),
        );
        let italic_angle = if face.slot == F_ITALIC { -12 } else { 0 };
        emit(
            &mut buf,
            &mut offsets,
            desc_obj(k),
            &format!(
                "<< /Type /FontDescriptor /FontName /{psname} /Flags 4 \
                 /FontBBox [{bx0} {by0} {bx1} {by1}] /ItalicAngle {italic_angle} \
                 /Ascent {asc} /Descent {desc} /CapHeight {cap} /StemV 80 /FontFile2 {ff} 0 R >>",
                bx0 = -200,
                by0 = m.descent - 50,
                bx1 = 1100,
                by1 = m.ascent + 50,
                asc = m.ascent,
                desc = m.descent,
                cap = m.cap_height,
                ff = file_obj(k),
            ),
        );
        // FontFile2: raw subset bytes (binary stream).
        offsets[file_obj(k)] = buf.len();
        buf.extend_from_slice(
            format!(
                "{n} 0 obj\n<< /Length {len} /Length1 {len} >>\nstream\n",
                n = file_obj(k),
                len = face.bytes.len(),
            )
            .as_bytes(),
        );
        buf.extend_from_slice(&face.bytes);
        buf.extend_from_slice(b"\nendstream\nendobj\n");
        // ToUnicode CMap.
        let cmap = tounicode_cmap(&face.font, &face.lig_uni);
        offsets[touni_obj(k)] = buf.len();
        buf.extend_from_slice(
            format!(
                "{n} 0 obj\n<< /Length {len} >>\nstream\n{cmap}endstream\nendobj\n",
                n = touni_obj(k),
                len = cmap.len(),
            )
            .as_bytes(),
        );
    }

    if !title.is_empty() {
        emit(
            &mut buf,
            &mut offsets,
            info_obj,
            &format!("<< /Title ({}) >>", pdf_escape(&title)),
        );
    }

    // xref + trailer.
    let xref_pos = buf.len();
    let size = total_objs + 1;
    buf.extend_from_slice(format!("xref\n0 {size}\n0000000000 65535 f \n").as_bytes());
    for offset in offsets.iter().take(total_objs + 1).skip(1) {
        buf.extend_from_slice(format!("{offset:010} 00000 n \n").as_bytes());
    }
    let info = if title.is_empty() {
        String::new()
    } else {
        format!(" /Info {info_obj} 0 R")
    };
    buf.extend_from_slice(
        format!("trailer\n<< /Size {size} /Root 1 0 R{info} >>\nstartxref\n{xref_pos}\n%%EOF\n")
            .as_bytes(),
    );
    buf
}

/// FontDescriptor metrics in 1/1000 em.
struct FaceMetrics {
    ascent: i32,
    descent: i32,
    cap_height: i32,
}

impl FaceMetrics {
    fn of(font: &Font) -> Self {
        let upm = font.units_per_em.max(1) as i32;
        let scale = |v: i32| v * 1000 / upm;
        Self {
            ascent: scale(font.ascent as i32),
            descent: scale(font.descent as i32),
            cap_height: scale((font.ascent as i32 * 7) / 10),
        }
    }
}

/// `/W` widths array `[ 0 [w0 w1 ...] ]` (1/1000 em, indexed by glyph id = CID).
fn widths_array(font: &Font) -> String {
    let upm = font.units_per_em.max(1) as u32;
    let mut s = String::from("0 [");
    for gid in 0..font.num_glyphs {
        let w = font.advance_width(gid) as u32 * 1000 / upm;
        s.push_str(&w.to_string());
        s.push(' ');
    }
    s.push(']');
    s
}

/// Shape `text` with `source`'s ligatures, returning the shaped SOURCE glyph ids
/// and, for each emitted ligature, its source characters (so a `ToUnicode` entry
/// can keep the ligated text selectable).
fn shape(source: &Font, lig: &Ligatures, text: &str) -> (Vec<u16>, Vec<(u16, String)>) {
    let chars: Vec<char> = text.chars().collect();
    let gids: Vec<u16> = chars.iter().map(|&c| source.glyph_index(c)).collect();
    let mut shaped = Vec::with_capacity(gids.len());
    let mut lig_uni = Vec::new();
    let mut ci = 0;
    for (gid, count) in lig.substitute_with_spans(&gids) {
        shaped.push(gid);
        if count > 1 {
            let s: String = chars.get(ci..ci + count).unwrap_or(&[]).iter().collect();
            lig_uni.push((gid, s));
        }
        ci += count;
    }
    (shaped, lig_uni)
}

/// Build a `TJ` array (without the trailing `TJ`) from a pre-shaped SOURCE glyph
/// sequence: each glyph is emitted as its subset id via `map`, with GPOS pair
/// kerning (looked up on the original ids) inserted between glyphs.
fn kerned_tj(map: &BTreeMap<u16, u16>, source: &Font, kern: &Kerning, shaped: &[u16]) -> String {
    let upm = i32::from(source.units_per_em.max(1));
    let mut out = String::from("[<");
    for (i, &g) in shaped.iter().enumerate() {
        out.push_str(&format!("{:04X}", map.get(&g).copied().unwrap_or(0)));
        if let Some(&next) = shaped.get(i + 1) {
            let k = kern.pair(g, next);
            if k != 0 {
                // A TJ number shifts the next glyph left by number/1000 em, so a
                // tightening (negative) kern becomes a positive number.
                let adj = -(i32::from(k) * 1000 / upm);
                out.push_str(&format!(">{adj}<"));
            }
        }
    }
    out.push_str(">]");
    out
}

/// A `ToUnicode` CMap mapping each glyph id back to its character(s), so text
/// stays selectable. Only the glyphs the document uses appear.
fn tounicode_cmap(font: &Font, lig_uni: &BTreeMap<u16, String>) -> String {
    // (gid, UTF-16BE hex) over the chars present in the subset cmap, plus the
    // ligature glyphs (which no character maps to) so ligated text stays
    // selectable.
    let mut entries: Vec<(u16, String)> = Vec::new();
    for cp in 0x20u32..0x2C00 {
        if let Some(c) = char::from_u32(cp) {
            let g = font.glyph_index(c);
            if g != 0 {
                entries.push((g, utf16be_hex(c)));
            }
        }
    }
    for (g, s) in lig_uni {
        entries.push((*g, s.chars().map(utf16be_hex).collect()));
    }
    entries.sort_by_key(|&(g, _)| g);
    entries.dedup_by_key(|(g, _)| *g);

    let mut body = String::new();
    for chunk in entries.chunks(100) {
        body.push_str(&format!("{} beginbfchar\n", chunk.len()));
        for (g, hex) in chunk {
            body.push_str(&format!("<{g:04X}> <{hex}>\n"));
        }
        body.push_str("endbfchar\n");
    }
    format!(
        "/CIDInit /ProcSet findresource begin\n12 dict begin\nbegincmap\n\
         /CIDSystemInfo << /Registry (Adobe) /Ordering (UCS) /Supplement 0 >> def\n\
         /CMapName /Adobe-Identity-UCS def\n/CMapType 2 def\n\
         1 begincodespacerange\n<0000> <FFFF>\nendcodespacerange\n\
         {body}endcmap\nCMapName currentdict /CMap defineresource pop\nend\nend\n"
    )
}

fn utf16be_hex(c: char) -> String {
    let mut s = String::new();
    let mut buf = [0u16; 2];
    for u in c.encode_utf16(&mut buf) {
        s.push_str(&format!("{u:04X}"));
    }
    s
}

/// Deterministic subset PostScript name, e.g. `FMDFA1+Embedded`.
fn subset_psname(k: usize, slot: u8) -> String {
    let tag: String = (0..6)
        .map(|i| (b'A' + ((k as u8 + slot + i as u8) % 26)) as char)
        .collect();
    format!("{tag}+Embedded")
}

/// A minimal but valid empty single-page PDF (degenerate fallback).
fn empty_pdf() -> Vec<u8> {
    let mut buf: Vec<u8> = Vec::new();
    let mut offsets = [0usize; 4];
    buf.extend_from_slice(b"%PDF-1.7\n%\xE2\xE3\xCF\xD3\n");
    for (n, body) in [
        "<< /Type /Catalog /Pages 2 0 R >>",
        "<< /Type /Pages /Count 1 /Kids [3 0 R] >>",
        &format!("<< /Type /Page /Parent 2 0 R /MediaBox [0 0 {PAGE_W:.0} {PAGE_H:.0}] >>"),
    ]
    .into_iter()
    .enumerate()
    {
        offsets[n + 1] = buf.len();
        buf.extend_from_slice(format!("{} 0 obj\n{body}\nendobj\n", n + 1).as_bytes());
    }
    let xref_pos = buf.len();
    buf.extend_from_slice(b"xref\n0 4\n0000000000 65535 f \n");
    for off in offsets.iter().skip(1) {
        buf.extend_from_slice(format!("{off:010} 00000 n \n").as_bytes());
    }
    buf.extend_from_slice(
        format!("trailer\n<< /Size 4 /Root 1 0 R >>\nstartxref\n{xref_pos}\n%%EOF\n").as_bytes(),
    );
    buf
}

// ---- text helpers -----------------------------------------------------------

fn pdf_escape(s: &str) -> String {
    let mut o = String::with_capacity(s.len() + 4);
    for c in s.chars() {
        match c {
            '(' => o.push_str("\\("),
            ')' => o.push_str("\\)"),
            '\\' => o.push_str("\\\\"),
            '\r' => o.push_str("\\r"),
            '\n' => o.push(' '),
            c if (c as u32) < 256 => o.push(c),
            _ => o.push('?'),
        }
    }
    o
}

/// Greedy word-wrap to a max width (points) using the face's real metrics.
fn wrap(text: &str, max_width: f32, size: f32, font: u8, faces: &Faces) -> Vec<String> {
    let mut lines = Vec::new();
    let mut cur = String::new();
    let mut cur_w = 0.0_f32;
    let sw = text_width(" ", size, font, faces);
    for word in text.split_whitespace() {
        let ww = text_width(word, size, font, faces);
        if !cur.is_empty() && cur_w + sw + ww > max_width {
            lines.push(std::mem::take(&mut cur));
            cur_w = 0.0;
        }
        if cur.is_empty() {
            cur.push_str(word);
            cur_w = ww;
        } else {
            cur.push(' ');
            cur.push_str(word);
            cur_w += sw + ww;
        }
    }
    if !cur.is_empty() {
        lines.push(cur);
    }
    if lines.is_empty() {
        lines.push(String::new());
    }
    lines
}

fn clip_to_width(text: &str, max_width: f32, size: f32, font: u8, faces: &Faces) -> String {
    if text_width(text, size, font, faces) <= max_width {
        return text.to_string();
    }
    let mut out = String::new();
    let mut w = 0.0;
    for c in text.chars() {
        let cw = faces.advance(font, c) * size / 1000.0;
        if w + cw > max_width {
            break;
        }
        out.push(c);
        w += cw;
    }
    out
}

fn text_width(s: &str, size: f32, font: u8, faces: &Faces) -> f32 {
    s.chars().map(|c| faces.advance(font, c)).sum::<f32>() * size / 1000.0
}

fn inline_text(inlines: &[Inline]) -> String {
    let mut s = String::new();
    for inl in inlines {
        match inl {
            Inline::Text(t) | Inline::Code(t) => s.push_str(t),
            Inline::Emphasis(c) | Inline::Strong(c) | Inline::Strikethrough(c) => {
                s.push_str(&inline_text(c));
            }
            Inline::Link { content, .. } => s.push_str(&inline_text(content)),
            Inline::Image { alt, .. } => s.push_str(alt),
            Inline::SoftBreak | Inline::HardBreak => s.push(' '),
            Inline::Html(_) => {}
        }
    }
    s
}

fn block_plain(block: &Block) -> String {
    match block {
        Block::Paragraph(inl) | Block::Heading { inlines: inl, .. } => inline_text(inl),
        Block::CodeBlock { code, .. } => code.clone(),
        _ => String::new(),
    }
}
