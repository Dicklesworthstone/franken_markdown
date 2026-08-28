//! PDF/A-2b plumbing: compact sRGB ICC, XMP identification packet, OutputIntent.
//!
//! Default renders stay off this path so PDF bytes remain identical. Enabling
//! [`PdfASettings::a2b`] appends three objects after Info/SMask (no renumber
//! of existing objects) and points the Catalog at them.

use crate::error::{RenderError, Result};

/// PDF/A profile selection. `Off` is the default renderer path.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PdfAMode {
    /// Do not emit PDF/A identification. Byte-identical with historical output.
    #[default]
    Off,
    /// PDF/A-2b: visual reproducibility, XMP + sRGB OutputIntent.
    A2b,
}

impl PdfAMode {
    /// Parse CLI/API spelling: `2b`, `pdf-a-2b`, `PDF/A-2b`.
    #[must_use]
    pub fn parse(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "off" | "none" | "0" => Some(Self::Off),
            "2b" | "a2b" | "pdf-a-2b" | "pdf/a-2b" | "pdfa-2b" => Some(Self::A2b),
            _ => None,
        }
    }

    /// Stable spelling for capabilities / JSON.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Off => "off",
            Self::A2b => "2b",
        }
    }

    /// True when XMP + OutputIntent objects should be emitted.
    #[must_use]
    pub const fn is_a2b(self) -> bool {
        matches!(self, Self::A2b)
    }
}

/// Engine-side PDF/A request. Kept off [`crate::PdfOptions`] so existing
/// complete struct literals (CLI, batch) compile while `src/cli.rs` is locked.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct PdfASettings {
    /// Profile to emit. [`PdfAMode::Off`] is a no-op.
    pub mode: PdfAMode,
    /// When true, non-conformable constructs return [`RenderError::InvalidInput`]
    /// with a named `pdf_a_*` code instead of being dropped.
    pub strict: bool,
}

impl PdfASettings {
    /// Default: no PDF/A objects, no strict checks.
    pub const OFF: Self = Self {
        mode: PdfAMode::Off,
        strict: false,
    };

    /// PDF/A-2b emission, non-strict (forbidden URI actions are dropped).
    #[must_use]
    pub const fn a2b() -> Self {
        Self {
            mode: PdfAMode::A2b,
            strict: false,
        }
    }

    /// PDF/A-2b emission, fail closed on non-conformable constructs.
    #[must_use]
    pub const fn a2b_strict() -> Self {
        Self {
            mode: PdfAMode::A2b,
            strict: true,
        }
    }

    /// Extra PDF objects appended after Info/SMask when this profile is on.
    #[must_use]
    pub const fn extra_object_count(self) -> usize {
        if self.mode.is_a2b() { 3 } else { 0 }
    }
}

/// True when a URI action is forbidden in PDF/A-2b (`javascript:`, `file:`).
#[must_use]
pub fn uri_forbidden_in_pdfa(uri: &str) -> bool {
    let t = uri.trim();
    let head = t.get(..11).unwrap_or(t);
    let lower = head.to_ascii_lowercase();
    lower.starts_with("javascript:") || lower.starts_with("file:")
}

/// Named strict-mode rejection. `code` is stable for robot/JSON output.
pub fn pdfa_reject(code: &'static str, message: &str) -> RenderError {
    RenderError::InvalidInput(format!("{code}: {message}"))
}

/// Walk URI annotations. Strict mode errors; non-strict drops forbidden actions
/// by returning `true` (= caller should omit the `/A` URI action).
pub fn check_uri_action(settings: PdfASettings, uri: &str) -> Result<bool> {
    if !settings.mode.is_a2b() || !uri_forbidden_in_pdfa(uri) {
        return Ok(false);
    }
    if settings.strict {
        let kind = if uri.trim().to_ascii_lowercase().starts_with("javascript:") {
            "pdf_a_javascript_uri"
        } else {
            "pdf_a_file_uri"
        };
        return Err(pdfa_reject(
            kind,
            "PDF/A-2b forbids javascript: and file: URI actions; remove the link or omit --pdf-a-strict",
        ));
    }
    Ok(true)
}

/// Catalog extras: Metadata stream + OutputIntents array.
#[must_use]
pub fn catalog_extras(metadata_obj: usize, output_intent_obj: usize) -> String {
    format!(" /Metadata {metadata_obj} 0 R /OutputIntents [ {output_intent_obj} 0 R ]")
}

/// `/OutputIntent` dictionary body (no wrapping `<< >>`? include them).
#[must_use]
pub fn output_intent_body(icc_obj: usize) -> String {
    format!(
        "<< /Type /OutputIntent /S /GTS_PDFA1 \
         /OutputConditionIdentifier (sRGB) \
         /Info (sRGB IEC61966-2.1 compact CC0) \
         /DestOutputProfile {icc_obj} 0 R >>"
    )
}

/// Deterministic PDF/A-2b XMP packet. Dates use `epoch` when set, else a
/// fixed Unix 0 so output stays reproducible without env.
#[must_use]
pub fn xmp_packet(title: &str, author: &str, epoch: Option<u64>) -> Vec<u8> {
    let date = xmp_date(epoch.unwrap_or(0));
    let mut xml = String::from(
        r#"<?xpacket begin="" id="W5M0MpCehiHzreSzNTczkc9d"?>
<x:xmpmeta xmlns:x="adobe:ns:meta/">
 <rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#">
  <rdf:Description rdf:about=""
    xmlns:pdfaid="http://www.aiim.org/pdfa/ns/id/"
    xmlns:xmp="http://ns.adobe.com/xap/1.0/"
    xmlns:pdf="http://ns.adobe.com/pdf/1.3/"
    xmlns:dc="http://purl.org/dc/elements/1.1/">
   <pdfaid:part>2</pdfaid:part>
   <pdfaid:conformance>B</pdfaid:conformance>
"#,
    );
    xml.push_str("   <xmp:CreateDate>");
    xml.push_str(&date);
    xml.push_str("</xmp:CreateDate>\n   <xmp:ModifyDate>");
    xml.push_str(&date);
    xml.push_str("</xmp:ModifyDate>\n   <xmp:CreatorTool>fmd</xmp:CreatorTool>\n");
    xml.push_str("   <pdf:Producer>franken_markdown</pdf:Producer>\n");
    if !title.is_empty() {
        xml.push_str("   <dc:title><rdf:Alt><rdf:li xml:lang=\"x-default\">");
        push_xml_escaped(&mut xml, title);
        xml.push_str("</rdf:li></rdf:Alt></dc:title>\n");
    }
    if !author.is_empty() {
        xml.push_str("   <dc:creator><rdf:Seq><rdf:li>");
        push_xml_escaped(&mut xml, author);
        xml.push_str("</rdf:li></rdf:Seq></dc:creator>\n");
    }
    xml.push_str(
        r#"  </rdf:Description>
 </rdf:RDF>
</x:xmpmeta>
<?xpacket end="w"?>
"#,
    );
    xml.into_bytes()
}

fn xmp_date(epoch: u64) -> String {
    // Deterministic civil date from Unix seconds, UTC, no leap-second table.
    let secs = epoch.min(4_102_444_800); // cap ~2100
    let days = secs / 86400;
    let rem = secs % 86400;
    let hour = rem / 3600;
    let min = (rem % 3600) / 60;
    let sec = rem % 60;
    let (year, month, day) = civil_from_days(days);
    format!("{year:04}-{month:02}-{day:02}T{hour:02}:{min:02}:{sec:02}Z")
}

fn civil_from_days(unix_days: u64) -> (u32, u32, u32) {
    // Howard Hinnant's civil_from_days, Unix epoch 1970-01-01 = day 0.
    let z = i64::try_from(unix_days).unwrap_or(i64::MAX) + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    (y as u32, m as u32, d as u32)
}

fn push_xml_escaped(out: &mut String, s: &str) {
    for ch in s.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            c => out.push(c),
        }
    }
}

/// Compact sRGB v2 ICC (monitor class, RGB, D50 PCS).
///
/// Provenance: project-authored, CC0. Primaries are IEC 61966-2-1 sRGB
/// Bradford-adapted to D50; TRC is a single-entry gamma 2.2 `curv` (not the
/// full sRGB piecewise function). This is an OutputIntent identifier profile,
/// not a color.org `sRGB2014.icc` binary.
#[must_use]
pub fn compact_srgb_icc() -> Vec<u8> {
    // Tag table: desc, cprt, wtpt, rXYZ, gXYZ, bXYZ, rTRC, gTRC, bTRC.
    const TAG_COUNT: u32 = 9;
    const HEADER: usize = 128;
    const TAG_ENTRY: usize = 12;
    let tag_dir = 4 + TAG_COUNT as usize * TAG_ENTRY; // 112
    let data_off = HEADER + tag_dir; // 240

    let desc = desc_tag(b"fmd compact sRGB");
    let cprt = text_tag(b"CC0-1.0 franken_markdown compact sRGB");
    let wtpt = xyz_tag(0.9642, 1.0, 0.8249);
    // Bradford-adapted sRGB primaries (IEC 61966-2-1) to D50.
    let rxyz = xyz_tag(0.436_065_673, 0.222_488_403, 0.013_916_016);
    let gxyz = xyz_tag(0.385_147_095, 0.716_873_169, 0.097_076_416);
    let bxyz = xyz_tag(0.143_066_406, 0.060_607_910, 0.714_096_069);
    let trc = curv_gamma_22();

    let tags: [([u8; 4], Vec<u8>); 9] = [
        (*b"desc", desc),
        (*b"cprt", cprt),
        (*b"wtpt", wtpt),
        (*b"rXYZ", rxyz),
        (*b"gXYZ", gxyz),
        (*b"bXYZ", bxyz),
        (*b"rTRC", trc.clone()),
        (*b"gTRC", trc.clone()),
        (*b"bTRC", trc),
    ];

    let mut offsets = Vec::with_capacity(9);
    let mut cursor = data_off;
    for (_, data) in &tags {
        let aligned = (cursor + 3) & !3;
        offsets.push(aligned);
        cursor = aligned + data.len();
    }
    let total = (cursor + 3) & !3;

    let mut out = vec![0u8; total];
    out[0..4].copy_from_slice(&(total as u32).to_be_bytes());
    out[4..8].copy_from_slice(b"NONE");
    out[8..12].copy_from_slice(&0x0210_0000u32.to_be_bytes()); // v2.1.0
    out[12..16].copy_from_slice(b"mntr");
    out[16..20].copy_from_slice(b"RGB ");
    out[20..24].copy_from_slice(b"XYZ ");
    // profile date 2026-01-01 00:00:00
    out[24..36].copy_from_slice(&[
        0, 7, 0xEA, 0, 1, 0, 1, 0, 0, 0, 0, 0,
    ]);
    out[36..40].copy_from_slice(b"acsp");
    // illuminant D50 in header (s15Fixed16)
    write_s15f16(&mut out[68..72], 0.9642);
    write_s15f16(&mut out[72..76], 1.0);
    write_s15f16(&mut out[76..80], 0.8249);
    out[80..84].copy_from_slice(b"fmd ");

    let dir = HEADER;
    out[dir..dir + 4].copy_from_slice(&TAG_COUNT.to_be_bytes());
    for (i, (sig, data)) in tags.iter().enumerate() {
        let e = dir + 4 + i * TAG_ENTRY;
        out[e..e + 4].copy_from_slice(sig);
        out[e + 4..e + 8].copy_from_slice(&(offsets[i] as u32).to_be_bytes());
        out[e + 8..e + 12].copy_from_slice(&(data.len() as u32).to_be_bytes());
        out[offsets[i]..offsets[i] + data.len()].copy_from_slice(data);
    }
    out
}

fn desc_tag(ascii: &[u8]) -> Vec<u8> {
    let mut t = Vec::from(*b"desc");
    t.extend_from_slice(&[0, 0, 0, 0]);
    let n = ascii.len() + 1;
    t.extend_from_slice(&(n as u32).to_be_bytes());
    t.extend_from_slice(ascii);
    t.push(0);
    t.extend_from_slice(&[0, 0, 0, 0, 0, 0, 0, 0]); // unicode
    t.extend_from_slice(&[0, 0, 0]); // scriptcode count 0 + 67 mac bytes
    t.extend_from_slice(&[0u8; 67]);
    t
}

fn text_tag(ascii: &[u8]) -> Vec<u8> {
    let mut t = Vec::from(*b"text");
    t.extend_from_slice(&[0, 0, 0, 0]);
    t.extend_from_slice(ascii);
    t.push(0);
    t
}

fn xyz_tag(x: f64, y: f64, z: f64) -> Vec<u8> {
    let mut t = Vec::from(*b"XYZ ");
    t.extend_from_slice(&[0, 0, 0, 0]);
    t.extend_from_slice(&s15f16(x));
    t.extend_from_slice(&s15f16(y));
    t.extend_from_slice(&s15f16(z));
    t
}

fn curv_gamma_22() -> Vec<u8> {
    let mut t = Vec::from(*b"curv");
    t.extend_from_slice(&[0, 0, 0, 0]);
    t.extend_from_slice(&1u32.to_be_bytes());
    // u8Fixed8 2.2 ≈ 2 + 51/256 = 563
    t.extend_from_slice(&563u16.to_be_bytes());
    t
}

fn s15f16(v: f64) -> [u8; 4] {
    let n = (v * 65536.0).round() as i32;
    n.to_be_bytes()
}

fn write_s15f16(out: &mut [u8], v: f64) {
    out.copy_from_slice(&s15f16(v));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn log_check(id: &str, subject: &str, ok: bool) {
        eprintln!(
            "check id={id} subject={subject} outcome={}",
            if ok { "PASS" } else { "FAIL" }
        );
        assert!(ok, "{id}: {subject}");
    }

    #[test]
    fn icc_header_size_matches_buffer() {
        let icc = compact_srgb_icc();
        let declared = u32::from_be_bytes(icc[0..4].try_into().unwrap()) as usize;
        log_check(
            "q6xc.1.icc.size",
            "header size equals buffer length",
            declared == icc.len() && icc.len() > 128,
        );
        log_check(
            "q6xc.1.icc.magic",
            "acsp magic at byte 36",
            &icc[36..40] == b"acsp",
        );
        log_check(
            "q6xc.1.icc.class",
            "mntr RGB XYZ",
            &icc[12..24] == b"mntrRGB XYZ ",
        );
    }

    #[test]
    fn xmp_identifies_pdfa_2b_and_is_deterministic() {
        let a = xmp_packet("Hi", "A", Some(1_700_000_000));
        let b = xmp_packet("Hi", "A", Some(1_700_000_000));
        let s = String::from_utf8(a.clone()).unwrap();
        log_check("q6xc.1.xmp.det", "same inputs same bytes", a == b);
        log_check(
            "q6xc.1.xmp.part",
            "pdfaid:part 2",
            s.contains("<pdfaid:part>2</pdfaid:part>"),
        );
        log_check(
            "q6xc.1.xmp.conf",
            "pdfaid:conformance B",
            s.contains("<pdfaid:conformance>B</pdfaid:conformance>"),
        );
        log_check(
            "q6xc.1.xmp.escape",
            "title is XML-escaped",
            !xmp_packet("a&b", "", Some(0))
                .windows(3)
                .any(|w| w == b"a&b")
                && String::from_utf8(xmp_packet("a&b", "", Some(0)).to_vec())
                    .unwrap()
                    .contains("a&amp;b"),
        );
    }

    #[test]
    fn forbidden_uri_matrix() {
        log_check(
            "q6xc.1.uri.js",
            "javascript: forbidden",
            uri_forbidden_in_pdfa("javascript:alert(1)"),
        );
        log_check(
            "q6xc.1.uri.file",
            "file: forbidden",
            uri_forbidden_in_pdfa("file:///etc/passwd"),
        );
        log_check(
            "q6xc.1.uri.https",
            "https allowed",
            !uri_forbidden_in_pdfa("https://example.com"),
        );
        let err = check_uri_action(PdfASettings::a2b_strict(), "javascript:x").unwrap_err();
        log_check(
            "q6xc.1.strict.js",
            "strict javascript: is pdf_a_javascript_uri",
            err.to_string().contains("pdf_a_javascript_uri"),
        );
        log_check(
            "q6xc.1.nonstrict.drop",
            "non-strict drops the action",
            check_uri_action(PdfASettings::a2b(), "file://x").unwrap(),
        );
    }
}
