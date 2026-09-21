//! Bounded data-only asset batches for the WASM seam. The native transaction
//! stays in FlowSession. Metadata is canonical decimal fields, not JSON/code;
//! payload lengths describe consecutive exact slices of one owned JS snapshot.

use super::{
    AssetRequestId, AssetResult, BrowserFlowError, BrowserFlowSession,
    FlowDisplayError, FlowSessionError, MAX_BROWSER_ASSET_BYTES, parse_identity,
};
use std::collections::HashSet;

const MAX_BATCH_COUNT: usize = 1024;
const MAX_BATCH_BYTES: usize = 32 * 1024 * 1024;
const MAX_METADATA_BYTES: usize = 96 * 1024;
const MAGIC: &str = "fmd-assets-v1\n";

struct Descriptor {
    id: AssetRequestId,
    generation: u64,
    width: u32,
    height: u32,
    bytes: Option<std::ops::Range<usize>>,
}

fn budget(name: &str) -> BrowserFlowError {
    FlowSessionError::Input(FlowDisplayError::BudgetExceeded(name.to_owned())).into()
}

fn descriptors(metadata: &str, payload_len: usize) -> Result<Vec<Descriptor>, BrowserFlowError> {
    if metadata.len() > MAX_METADATA_BYTES || payload_len > MAX_BATCH_BYTES {
        return Err(budget("asset batch transport"));
    }
    let records = metadata.strip_prefix(MAGIC).ok_or(BrowserFlowError::InvalidAssetBatch)?;
    if records.is_empty() {
        return if payload_len == 0 { Ok(Vec::new()) } else { Err(BrowserFlowError::InvalidAssetBatch) };
    }
    let mut out = Vec::new();
    let mut offset = 0usize;
    for row in records.split('\n') {
        if out.len() == MAX_BATCH_COUNT { return Err(budget("asset batch count")); }
        let mut fields = row.split(',');
        let id = AssetRequestId(parse_identity(fields.next().ok_or(BrowserFlowError::InvalidAssetBatch)?)?);
        let generation = parse_identity(fields.next().ok_or(BrowserFlowError::InvalidAssetBatch)?)?;
        let width = u32::try_from(parse_identity(fields.next().ok_or(BrowserFlowError::InvalidAssetBatch)?)?)
            .map_err(|_| BrowserFlowError::InvalidAssetBatch)?;
        let height = u32::try_from(parse_identity(fields.next().ok_or(BrowserFlowError::InvalidAssetBatch)?)?)
            .map_err(|_| BrowserFlowError::InvalidAssetBatch)?;
        let length = fields.next().ok_or(BrowserFlowError::InvalidAssetBatch)?;
        if fields.next().is_some() { return Err(BrowserFlowError::InvalidAssetBatch); }
        let bytes = if length == "-" { None } else {
            let len = usize::try_from(parse_identity(length)?)
                .map_err(|_| BrowserFlowError::InvalidAssetBatch)?;
            if len > MAX_BROWSER_ASSET_BYTES { return Err(budget("asset bytes")); }
            let end = offset.checked_add(len).filter(|end| *end <= payload_len)
                .ok_or(BrowserFlowError::InvalidAssetBatch)?;
            let range = offset..end;
            offset = end;
            Some(range)
        };
        out.push(Descriptor { id, generation, width, height, bytes });
    }
    // Extra, omitted, aliased and overlapping payload data are not representable.
    if offset != payload_len { return Err(BrowserFlowError::InvalidAssetBatch); }
    Ok(out)
}

impl BrowserFlowSession {
    /// Apply a native vector of image completions with one atomic layout update.
    /// Empty input is a no-op. IDs, dimensions and budgets use the same engine
    /// rules as provide_asset; no image is fetched or decoded by this operation.
    pub fn provide_assets(&mut self, assets: Vec<AssetResult>) -> Result<(), BrowserFlowError> {
        let fonts = &mut self.fonts;
        self.session.provide_assets(assets, |text, size, role, style| fonts.shape(text, size, role, style))?;
        Ok(())
    }

    /// Internal bounded ABI: fmd-assets-v1\n followed by newline-separated
    /// requestId,generation,width,height,length records, with no trailing newline.
    /// `-` means no bytes; `0` means a present empty payload. All numbers are
    /// canonical decimal integers. The JS facade admits/copies before WASM ingress;
    /// WASM's own argument conversion necessarily occurs before these checks.
    pub(super) fn provide_assets_packed(&mut self, metadata: &str, payload: &[u8])
        -> Result<(), BrowserFlowError>
    {
        let items = descriptors(metadata, payload.len())?;
        let limits = self.session.engine().limits();
        let available = limits.max_retained_asset_bytes.saturating_sub(self.session.engine().retained_asset_bytes());
        if payload.len() > available { return Err(budget("retained asset bytes")); }
        let mut pending: HashSet<_> = self.session.pending_assets().iter().map(|request| request.id).collect();
        // Complete identity/dimension validation before copying ANY payload.
        for item in &items {
            if item.generation != self.revision() {
                return Err(FlowSessionError::Input(FlowDisplayError::StaleAssetGeneration {
                    expected: self.revision(), actual: item.generation,
                }).into());
            }
            if !pending.remove(&item.id) {
                return Err(FlowSessionError::Input(FlowDisplayError::UnknownAssetRequest(item.id)).into());
            }
            if item.width == 0 || item.height == 0
                || item.width > limits.max_image_dimension || item.height > limits.max_image_dimension
            {
                return Err(FlowSessionError::Input(FlowDisplayError::InvalidAssetDimensions {
                    width: item.width, height: item.height,
                }).into());
            }
        }
        let assets = items.into_iter().map(|item| AssetResult {
            request_id: item.id, generation: item.generation, width: item.width, height: item.height,
            bytes: item.bytes.map(|range| payload[range].to_vec()),
        }).collect();
        self.provide_assets(assets)
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]
    use super::*;
    use crate::flow_display::FlowLayoutOptions;

    #[test]
    fn packed_completion_preserves_binary_slices_empty_and_dimension_only_payloads() {
        let source = "before\n\n![a](a.png) ![b](b.png) ![c](c.png)\n\nafter";
        let mut state = BrowserFlowSession::new(source, "sans", FlowLayoutOptions::default()).unwrap();
        state.provide_assets_packed("fmd-assets-v1\n3,1,40,20,-\n1,1,20,10,4\n2,1,30,15,0",
            &[0, 255, 128, 10]).unwrap();
        assert_eq!(state.asset_bytes(1, 1).unwrap(), Some(&[0, 255, 128, 10][..]));
        assert_eq!(state.asset_bytes(2, 1).unwrap(), Some(&[][..]));
        assert_eq!(state.asset_bytes(3, 1).unwrap(), None);
        assert_eq!((state.revision(), state.layout_revision()), (1, 2));
        assert_eq!(state.source(), source);
        assert!(state.session.pending_assets().is_empty());
        state.provide_assets_packed(MAGIC, &[]).unwrap();
        assert_eq!(state.layout_revision(), 2);
    }

    #[test]
    fn descriptor_admission_rejects_malformed_lengths_fields_and_identities() {
        for input in ["", "fmd-assets-v2\n", "fmd-assets-v1\n\n", "fmd-assets-v1\n1,1,20,10",
            "fmd-assets-v1\n1,1,20,10,0,0", "fmd-assets-v1\n1,1,20,10,0\n",
            "fmd-assets-v1\n01,1,20,10,0", "fmd-assets-v1\n1,+1,20,10,0",
            "fmd-assets-v1\n1,1,20,10,00", "fmd-assets-v1\n1,1,20,10,1",
            "fmd-assets-v1\n18446744073709551616,1,20,10,0"] {
            assert!(descriptors(input, 0).is_err(), "{input:?}");
        }
        assert!(descriptors(MAGIC, 1).is_err());
        assert!(descriptors("fmd-assets-v1\n1,1,20,10,0", 1).is_err());
        assert!(descriptors("fmd-assets-v1\n1,1,20,10,8388609", 8388609).is_err());
        let items = descriptors("fmd-assets-v1\n9007199254740993,18446744073709551615,20,10,-", 0).unwrap();
        assert_eq!(items[0].id.0, 9_007_199_254_740_993);
        assert_eq!(items[0].generation, u64::MAX);
    }

    #[test]
    fn structural_budgets_apply_before_payload_copy() {
        let rows = (0..1025).map(|i| format!("{i},1,1,1,-")).collect::<Vec<_>>().join("\n");
        assert_eq!(descriptors(&format!("{MAGIC}{rows}"), 0).err().unwrap().code(), "BUDGET_EXCEEDED");
        assert_eq!(descriptors(&"x".repeat(MAX_METADATA_BYTES + 1), 0).err().unwrap().code(), "BUDGET_EXCEEDED");
        assert_eq!(descriptors(MAGIC, MAX_BATCH_BYTES + 1).err().unwrap().code(), "BUDGET_EXCEEDED");
    }

    #[test]
    fn a_bad_tail_record_cannot_consume_an_earlier_request() {
        for record in ["1,1,20,10,0", "2,2,20,10,0", "9,1,20,10,0", "2,1,0,10,0"] {
            let mut state = BrowserFlowSession::new("![a](a.png) ![b](b.png)", "sans", FlowLayoutOptions::default()).unwrap();
            let before = state.reading_json(1, 1, 0, 100).unwrap();
            assert!(state.provide_assets_packed(&format!("{MAGIC}1,1,20,10,1\n{record}"), &[42]).is_err());
            assert_eq!(state.session.pending_assets().len(), 2);
            assert_eq!(state.session.engine().retained_asset_bytes(), 0);
            assert_eq!(state.reading_json(1, 1, 0, 100).unwrap(), before);
            state.provide_assets_packed("fmd-assets-v1\n1,1,20,10,1\n2,1,20,10,-", &[42]).unwrap();
            assert_eq!(state.layout_revision(), 2);
        }
    }
}
