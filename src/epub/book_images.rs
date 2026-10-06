//! Publication-wide image identity without duplicate payload ownership.
//!
//! Chapters are still prepared by the existing data-image extractor. This
//! export-local pool moves their resources, rewrites only generated img/src
//! attributes, and eventually gives each unique payload to its first chapter.

use std::collections::BTreeMap;
use std::ops::Range;

use super::resources::{Chapter as ImageChapter, Resource, tag_end};
use super::{MAX_IMAGE_BYTES, MAX_RESOURCES, Result, fnv1a64, invalid};

#[derive(Default)]
pub(super) struct SharedImages {
    resources: Vec<(usize, Resource)>,
    buckets: BTreeMap<(&'static str, usize, u64), Vec<usize>>,
    bytes: usize,
}

impl SharedImages {
    pub(super) fn prepare(&mut self, owner: usize, chapter: &mut ImageChapter) -> Result<()> {
        let mut aliases = BTreeMap::new();
        for resource in std::mem::take(&mut chapter.resources) {
            let old = resource.href.clone();
            let hash = fnv1a64(0xcbf2_9ce4_8422_2325, &resource.bytes);
            let canonical = self.intern(owner, resource, hash)?;
            if old != canonical {
                aliases.insert(old, canonical.to_string());
            }
        }
        if !aliases.is_empty() {
            chapter.body = rewrite_sources(&chapter.body, &aliases);
        }
        Ok(())
    }

    // A fingerprint only selects equality candidates; it NEVER establishes
    // identity. Equal-length collisions and MIME differences remain distinct.
    fn intern(&mut self, owner: usize, resource: Resource, hash: u64) -> Result<&str> {
        let key = (resource.media_type, resource.bytes.len(), hash);
        let existing = self.buckets.get(&key).and_then(|indices| {
            indices.iter().copied().find(|&index| {
                self.resources[index].1.bytes == resource.bytes
            })
        });
        if let Some(index) = existing {
            return Ok(&self.resources[index].1.href);
        }
        // Charge only newly retained bytes. An already admitted image can be
        // referenced again even when either unique-resource budget is full.
        if self.resources.len() >= MAX_RESOURCES
            || resource.bytes.len() > MAX_IMAGE_BYTES.saturating_sub(self.bytes)
        {
            return Err(invalid("book exceeds 4096 unique images or 128 MiB of image payloads"));
        }
        self.bytes += resource.bytes.len();
        let index = self.resources.len();
        self.resources.push((owner, resource));
        self.buckets.entry(key).or_default().push(index);
        Ok(&self.resources[index].1.href)
    }

    pub(super) fn byte_len(&self) -> usize {
        self.bytes
    }

    pub(super) fn finish(self) -> Vec<(usize, Resource)> {
        self.resources
    }
}

// Only source attributes in real image tags are changed. A global replacement
// would corrupt visible code, link targets, alt text and quoted tag-shaped text.
fn rewrite_sources(html: &str, aliases: &BTreeMap<String, String>) -> String {
    let mut output = String::with_capacity(html.len());
    let mut rest = html;
    while let Some(start) = rest.find('<') {
        output.push_str(&rest[..start]);
        rest = &rest[start..];
        let Some(end) = tag_end(rest) else {
            break;
        };
        let tag = &rest[..=end];
        let replacement = source_range(tag).and_then(|range| {
            aliases.get(&tag[range.clone()]).map(|href| (range, href))
        });
        if let Some((range, href)) = replacement {
            output.push_str(&tag[..range.start]);
            output.push_str(href);
            output.push_str(&tag[range.end..]);
        } else {
            output.push_str(tag);
        }
        rest = &rest[end + 1..];
    }
    output.push_str(rest);
    output
}

// Read the renderer's quoted attribute grammar, without interpreting entities
// or user HTML. The extractor's generated resource paths need no XML decoding.
fn source_range(tag: &str) -> Option<Range<usize>> {
    if !tag.strip_prefix("<img").and_then(|rest| rest.as_bytes().first())
        .is_some_and(u8::is_ascii_whitespace)
    {
        return None;
    }
    let bytes = tag.as_bytes();
    let mut index = 4;
    while index < bytes.len() {
        while bytes.get(index).is_some_and(u8::is_ascii_whitespace) {
            index += 1;
        }
        let name = index;
        while bytes.get(index).is_some_and(|byte| {
            !byte.is_ascii_whitespace() && !matches!(byte, b'=' | b'/' | b'>')
        }) {
            index += 1;
        }
        if index == name {
            return None;
        }
        let name = &tag[name..index];
        while bytes.get(index).is_some_and(u8::is_ascii_whitespace) {
            index += 1;
        }
        if bytes.get(index) != Some(&b'=') {
            continue;
        }
        index += 1;
        while bytes.get(index).is_some_and(u8::is_ascii_whitespace) {
            index += 1;
        }
        let quote = *bytes.get(index)?;
        if !matches!(quote, b'\'' | b'"') {
            return None;
        }
        index += 1;
        let start = index;
        while bytes.get(index).is_some_and(|byte| *byte != quote) {
            index += 1;
        }
        bytes.get(index)?;
        if name == "src" {
            return Some(start..index);
        }
        index += 1;
    }
    None
}

#[cfg(test)]
#[path = "book_image_tests.rs"]
mod tests;
