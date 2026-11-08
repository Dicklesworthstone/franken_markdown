//! Explicit native network preparation for a complete book. The shared book
//! renderer receives bytes only and never acquires network authority.

use std::collections::{BTreeMap, BTreeSet};

use super::{Failure, error};
use crate::PdfImageAsset;
use crate::book::Book;
use crate::cli::remote_images::{fetch_remote_image, remote_image_url, warning_message};

const MAX_IMAGES: usize = 4096;
const MAX_TOTAL_BYTES: usize = 128 * 1024 * 1024;

pub(super) fn load(
    book: &Book,
    images: &mut Vec<PdfImageAsset>,
    warnings: &mut Vec<String>,
    max_bytes: u64,
    timeout_secs: u64,
) -> Result<(), Failure> {
    // Keep the original source diagnostics associated with each URL. A
    // successfully fetched or explicitly mapped image is no longer missing;
    // a failed request receives one precise fetch diagnostic across chapters.
    let mut requests: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for chapter in &book.chapters {
        let lowered = crate::safe_html::lower(&chapter.doc);
        let mut destinations = Vec::new();
        crate::cli::collect_image_destinations(&lowered.blocks, &mut destinations);
        for destination in destinations {
            let Some(url) = remote_image_url(destination) else {
                continue;
            };
            requests
                .entry(url.to_owned())
                .or_default()
                .insert(format!("image_not_loaded: {}: {destination}", chapter.path));
            if requests.len() > MAX_IMAGES {
                return Err(error(
                    66,
                    "image_error",
                    "book exceeds 4096 remote image destinations",
                ));
            }
        }
    }
    let mut total: usize = images.iter().map(|asset| asset.bytes.len()).sum();
    for (url, source_warnings) in requests {
        warnings.retain(|warning| !source_warnings.contains(warning));
        if images.iter().any(|asset| asset.destination.trim() == url) {
            continue;
        }
        if images.len() >= MAX_IMAGES {
            return Err(error(
                66,
                "image_error",
                "book exceeds 4096 image resources",
            ));
        }
        match fetch_remote_image(&url, max_bytes.min(32 * 1024 * 1024), timeout_secs) {
            Ok(bytes) => {
                total = total.saturating_add(bytes.len());
                if total > MAX_TOTAL_BYTES {
                    return Err(error(
                        66,
                        "image_error",
                        "book image payloads exceed 128 MiB",
                    ));
                }
                images.push(PdfImageAsset::new(url, bytes));
            }
            Err(reason) => {
                if warnings.len() >= MAX_IMAGES {
                    return Err(error(
                        66,
                        "image_error",
                        "book image diagnostics exceed 4096 entries",
                    ));
                }
                warnings.push(format!(
                    "remote_image_fetch_failed: {}",
                    warning_message(&url, &reason)
                ));
            }
        }
    }
    Ok(())
}
