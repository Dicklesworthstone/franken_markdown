//! SVG-native hyperlinks over final painted geometry, not guessed source widths.
//! Measurement may intern metadata but never records regions or diagnostics.

use std::collections::{BTreeMap, BTreeSet};
use super::{Inline, SvgWarning, esc_attr, push_q2, push_u64_fast};

#[path = "link_targets.rs"]
mod targets;
use targets::{Target, heading_slug, inline_label, target, text_label};

const MAX_LINKS: usize = 4096;
const MAX_HEADINGS: usize = 4096;
const MAX_REGIONS: usize = 16_384;
const MAX_METADATA_BYTES: usize = 4 * 1024 * 1024;
const MAX_OUTPUT_BYTES: usize = 8 * 1024 * 1024;
const UNSAFE: usize = usize::MAX;
const LIMITED: usize = usize::MAX - 1;
const TOP: usize = usize::MAX - 2;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct Link {
    target: Target,
    title: String,
    label: String,
}

#[derive(Debug, Clone, Copy)]
struct Rect {
    left: f64,
    top: f64,
    right: f64,
    bottom: f64,
}

impl Rect {
    fn new((left, top, right, bottom): (f64, f64, f64, f64)) -> Option<Self> {
        if [left, top, right, bottom].iter().any(|v| !v.is_finite() || v.abs() > 1e9)
            || right <= left || bottom <= top
        {
            return None;
        }
        Some(Self { left, top, right, bottom })
    }

    fn clip(self, width: f64, height: f64) -> Option<Self> {
        Self::new((self.left.max(0.0), self.top.max(0.0),
            self.right.min(width), self.bottom.min(height)))
    }

    fn union(&mut self, other: Self) {
        self.left = self.left.min(other.left);
        self.top = self.top.min(other.top);
        self.right = self.right.max(other.right);
        self.bottom = self.bottom.max(other.bottom);
    }

    fn emit(self, out: &mut String) {
        // The same 0.01-point grid used by the glyph emitter; round the edges
        // outward so thin/short links never disappear through quantization.
        let left = (self.left * 100.0).floor() / 100.0;
        let top = (self.top * 100.0).floor() / 100.0;
        let right = (self.right * 100.0).ceil() / 100.0;
        let bottom = (self.bottom * 100.0).ceil() / 100.0;
        out.push_str("<rect x=\"");
        push_q2(out, left);
        out.push_str("\" y=\"");
        push_q2(out, top);
        out.push_str("\" width=\"");
        push_q2(out, right - left);
        out.push_str("\" height=\"");
        push_q2(out, bottom - top);
        out.push_str("\" fill=\"none\" pointer-events=\"all\"/></a>\n");
    }
}

#[derive(Debug)]
struct Region {
    link: usize,
    rect: Rect,
}

struct Heading {
    id: usize,
    top: f64,
    next_suffix: usize,
}

#[derive(Default)]
pub(super) struct Navigation {
    links: Vec<Link>,
    ids: BTreeMap<Link, usize>,
    headings: BTreeMap<String, Heading>,
    regions: Vec<Region>,
    seen: BTreeSet<usize>,
    metadata_bytes: usize,
    heading_limited: bool,
    region_limited: bool,
    geometry_adjusted: bool,
}

impl Navigation {
    pub(super) fn intern(&mut self, raw: &str, title: Option<&str>, content: &[Inline]) -> usize {
        if raw.len() > targets::MAX_TARGET_BYTES { return LIMITED; }
        let Some(target) = target(raw) else { return UNSAFE; };
        let mut label = inline_label(content);
        if label.trim().is_empty() { label = text_label(raw); }
        let link = Link { target, title: text_label(title.unwrap_or_default()), label };
        if let Some(&id) = self.ids.get(&link) { return id; }
        let destination_bytes = match &link.target {
            Target::External(value) | Target::Fragment(value) => value.len(),
        };
        // The map key and the ordered entry each own their metadata bytes.
        let bytes = (destination_bytes + link.title.len() + link.label.len()) * 2;
        if self.links.len() >= MAX_LINKS || bytes > MAX_METADATA_BYTES - self.metadata_bytes {
            return LIMITED;
        }
        self.metadata_bytes += bytes;
        let id = self.links.len();
        self.ids.insert(link.clone(), id);
        self.links.push(link);
        id
    }

    pub(super) fn heading(&mut self, inlines: &[Inline], top: f64) {
        // Once a heading cannot be named exactly, stop assigning later names:
        // otherwise skipped duplicates could retarget links to the wrong place.
        if self.heading_limited { return; }
        if self.headings.len() >= MAX_HEADINGS || !top.is_finite() || !(0.0..=1e9).contains(&top) {
            self.heading_limited = true;
            return;
        }
        let Some(base) = heading_slug(inlines) else {
            self.heading_limited = true;
            return;
        };
        let mut suffix = self.headings.get(&base).map_or(1, |h| h.next_suffix);
        let name = loop {
            let candidate = if suffix == 1 { base.clone() } else { format!("{base}-{suffix}") };
            suffix += 1;
            if !self.headings.contains_key(&candidate) { break candidate; }
        };
        if name.len() > MAX_METADATA_BYTES - self.metadata_bytes {
            self.heading_limited = true;
            return;
        }
        self.metadata_bytes += name.len();
        if let Some(heading) = self.headings.get_mut(&base) { heading.next_suffix = suffix; }
        let id = self.headings.len();
        let next_suffix = if name == base { suffix } else { 1 };
        self.headings.insert(name, Heading { id, top, next_suffix });
    }

    /// `previous` belongs to one draw_words invocation. A changed/no link
    /// clears it, so a hit region never covers unlinked words or another cell.
    pub(super) fn record(
        &mut self,
        link: Option<usize>,
        bounds: Option<(f64, f64, f64, f64)>,
        previous: &mut Option<usize>,
    ) {
        let Some(id) = link else { *previous = None; return; };
        self.seen.insert(id);
        if self.links.get(id).is_none() { *previous = None; return; }
        let Some(bounds) = bounds else { *previous = None; return; };
        let Some(rect) = Rect::new(bounds) else {
            // Zero-advance source (e.g. a missing combining mark) is not an
            // interactive rectangle; don't invent one over unrelated content.
            if bounds.2 != bounds.0 { self.geometry_adjusted = true; }
            *previous = None;
            return;
        };
        if *previous == Some(id)
            && let Some(last) = self.regions.last_mut().filter(|region| region.link == id)
        {
            last.rect.union(rect);
            return;
        }
        if self.regions.len() >= MAX_REGIONS {
            self.region_limited = true;
            *previous = None;
            return;
        }
        self.regions.push(Region { link: id, rect });
        *previous = Some(id);
    }

    pub(super) fn emit(self, out: &mut String, width: f64, height: f64, warnings: &mut Vec<SvgWarning>) {
        if self.seen.is_empty() { return; }
        let mut resolved = BTreeMap::new();
        let mut needed_views = BTreeSet::new();
        let mut unknown = false;
        for &id in &self.seen {
            let Some(link) = self.links.get(id) else { continue; };
            if let Target::Fragment(fragment) = &link.target {
                let view = if fragment.is_empty() { Some(TOP) }
                    else { self.headings.get(fragment).map(|heading| heading.id) };
                if let Some(view) = view {
                    resolved.insert(id, view);
                    needed_views.insert(view);
                } else { unknown = true; }
            }
        }
        let mut budget = MAX_OUTPUT_BYTES;
        let mut output_limited = false;
        let mut emitted_views = BTreeSet::new();
        let mut part = String::new();
        for heading in self.headings.values().filter(|h| needed_views.contains(&h.id)) {
            part.clear();
            emit_view(&mut part, heading.id, heading.top, width, height);
            if append_bounded(out, &part, &mut budget) { emitted_views.insert(heading.id); }
            else { output_limited = true; }
        }
        if needed_views.contains(&TOP) {
            part.clear();
            emit_view(&mut part, TOP, 0.0, width, height);
            if append_bounded(out, &part, &mut budget) { emitted_views.insert(TOP); }
            else { output_limited = true; }
        }
        for region in &self.regions {
            let Some(rect) = region.rect.clip(width, height) else { continue; };
            let link = &self.links[region.link];
            part.clear();
            part.push_str("<a href=\"");
            match &link.target {
                Target::External(uri) => esc_attr(uri, &mut part),
                Target::Fragment(_) => {
                    let Some(&view) = resolved.get(&region.link) else { continue; };
                    if !emitted_views.contains(&view) { continue; }
                    part.push('#');
                    emit_view_id(&mut part, view);
                }
            }
            part.push_str("\" target=\"_self\" rel=\"noopener noreferrer\" tabindex=\"0\" aria-label=\"");
            esc_attr(&link.label, &mut part);
            part.push_str("\">");
            if !link.title.is_empty() {
                part.push_str("<title>");
                esc_attr(&link.title, &mut part);
                part.push_str("</title>");
            }
            rect.emit(&mut part);
            if !append_bounded(out, &part, &mut budget) { output_limited = true; break; }
        }
        if self.seen.contains(&UNSAFE) {
            warn(warnings, "svg_link_unsafe", "Unsupported or unsafe link destinations were kept as inert visible content; only HTTP, HTTPS, mailto and known heading fragments are activated.");
        }
        if unknown {
            warn(warnings, "svg_link_unresolved", "One or more heading links have no matching exported heading; their visible content was retained without an active anchor.");
        }
        if self.seen.contains(&LIMITED) || self.region_limited || output_limited {
            warn(warnings, "svg_link_limit", "Navigation metadata, hit-region or output budget exceeded; affected links remain visible but inert.");
        }
        if self.heading_limited && self.links.iter().any(|link| matches!(&link.target, Target::Fragment(_))) {
            warn(warnings, "svg_anchor_limit", "Heading navigation budget exceeded; later heading IDs were not assigned, preventing ambiguous or renumbered destinations.");
        }
        if self.geometry_adjusted {
            warn(warnings, "svg_link_geometry", "Invalid link geometry was omitted without changing the painted content.");
        }
    }
}

fn append_bounded(out: &mut String, part: &str, remaining: &mut usize) -> bool {
    if part.len() > *remaining { return false; }
    *remaining -= part.len();
    out.push_str(part);
    true
}

fn emit_view_id(out: &mut String, id: usize) {
    if id == TOP { out.push_str("fmd-top"); }
    else { out.push_str("fmd-heading-"); push_u64_fast(out, id as u64); }
}

fn emit_view(out: &mut String, id: usize, top: f64, width: f64, height: f64) {
    // A named SVG view translates the origin to the heading without changing
    // scale: retain the root's width/height. Browser history or a `#` link
    // restores the overview. No script, event handler or external fetch.
    out.push_str("<view id=\"");
    emit_view_id(out, id);
    out.push_str("\" viewBox=\"0 ");
    push_q2(out, top);
    out.push(' ');
    push_q2(out, width);
    out.push(' ');
    push_q2(out, height);
    out.push_str("\" preserveAspectRatio=\"xMinYMin meet\"/>\n");
}

fn warn(warnings: &mut Vec<SvgWarning>, code: &'static str, message: &str) {
    warnings.push(SvgWarning { code, message: message.to_owned() });
}

#[cfg(test)]
#[path = "link_tests.rs"]
mod tests;
