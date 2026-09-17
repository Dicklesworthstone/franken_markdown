//! Feature-gated wasm-bindgen seam. All policy/transactions live in the native
//! BrowserFlowSession so the core remains testable without a JavaScript runtime.
use super::{BrowserFlowError, BrowserFlowSession, parse_identity, wire};
use crate::dep_invalidation::FlowAssetReuse;
use crate::flow_display::{AssetRequestId, AssetResult, FlowLayoutOptions};
use crate::SourceSpan;
use wasm_bindgen::prelude::*;

fn js_error(error: BrowserFlowError) -> JsValue { JsValue::from_str(&wire::error_json(&error)) }
fn id(value: &str) -> Result<u64, JsValue> { parse_identity(value).map_err(js_error) }
fn reuse(verified: bool) -> FlowAssetReuse {
    if verified { FlowAssetReuse::HostVerifiedUnchanged } else { FlowAssetReuse::Invalidate }
}
fn options(width: f32, body: f32, code: f32, leading: f32) -> FlowLayoutOptions {
    FlowLayoutOptions { viewport_width: width, body_size: body, code_size: code,
        line_height: leading, ..FlowLayoutOptions::default() }
}

/// Persistent browser editing and measured display output. The generated class
/// owns Rust allocations until free() (the public JS adapter calls dispose()).
#[wasm_bindgen]
pub struct FmdFlowSession { inner: BrowserFlowSession }

#[wasm_bindgen]
impl FmdFlowSession {
    #[wasm_bindgen(constructor)]
    pub fn new(source: &str, font: &str, width: f32, body: f32, code: f32, leading: f32)
        -> Result<FmdFlowSession, JsValue>
    {
        BrowserFlowSession::new(source, font, options(width, body, code, leading))
            .map(|inner| Self { inner }).map_err(js_error)
    }

    #[wasm_bindgen(getter)]
    pub fn revision(&self) -> String { self.inner.revision().to_string() }
    #[wasm_bindgen(getter, js_name = layoutRevision)]
    pub fn layout_revision(&self) -> String { self.inner.layout_revision().to_string() }
    #[wasm_bindgen(getter)]
    pub fn source(&self) -> String { self.inner.source().to_owned() }

    #[wasm_bindgen(js_name = editUtf16)]
    pub fn edit_utf16(&mut self, revision: &str, start: usize, end: usize,
        replacement: &str, verified_asset_reuse: bool) -> Result<(), JsValue>
    {
        self.inner.edit_utf16(id(revision)?, start, end, replacement, reuse(verified_asset_reuse)).map_err(js_error)
    }

    #[wasm_bindgen(js_name = editBytes)]
    pub fn edit_bytes(&mut self, revision: &str, start: usize, end: usize,
        replacement: &str, verified_asset_reuse: bool) -> Result<(), JsValue>
    {
        self.inner.edit_bytes(id(revision)?, SourceSpan::new(start, end), replacement,
            reuse(verified_asset_reuse)).map_err(js_error)
    }

    #[wasm_bindgen(js_name = replaceSource)]
    pub fn replace_source(&mut self, revision: &str, source: &str, verified_asset_reuse: bool) -> Result<(), JsValue> {
        self.inner.replace_source(id(revision)?, source, reuse(verified_asset_reuse)).map_err(js_error)
    }

    pub fn reflow(&mut self, revision: &str, layout_revision: &str, width: f32,
        body: f32, code: f32, leading: f32) -> Result<(), JsValue>
    {
        self.inner.reflow(id(revision)?, id(layout_revision)?, options(width, body, code, leading)).map_err(js_error)
    }

    #[wasm_bindgen(js_name = provideAsset)]
    pub fn provide_asset(&mut self, request_id: &str, generation: &str, width: u32,
        height: u32, bytes: Option<Vec<u8>>) -> Result<(), JsValue>
    {
        self.inner.provide_asset(AssetResult { request_id: AssetRequestId(id(request_id)?),
            generation: id(generation)?, width, height, bytes }).map_err(js_error)
    }

    #[wasm_bindgen(js_name = reloadAssets)]
    pub fn reload_assets(&mut self, revision: &str) -> Result<(), JsValue> {
        self.inner.reload_assets(id(revision)?).map_err(js_error)
    }

    #[wasm_bindgen(js_name = fontBytes)]
    pub fn font_bytes(&self, font_id: &str) -> Result<Vec<u8>, JsValue> {
        self.inner.font_bytes(id(font_id)?).map(<[u8]>::to_vec).map_err(js_error)
    }

    #[wasm_bindgen(js_name = assetBytes)]
    pub fn asset_bytes(&self, request_id: &str, revision: &str) -> Result<Option<Vec<u8>>, JsValue> {
        self.inner.asset_bytes(id(request_id)?, id(revision)?)
            .map(|bytes| bytes.map(<[u8]>::to_vec)).map_err(js_error)
    }

    #[wasm_bindgen(js_name = copySource)]
    pub fn copy_source(&self, revision: &str, start: usize, end: usize) -> Result<String, JsValue> {
        self.inner.copy_source(id(revision)?, SourceSpan::new(start, end)).map(str::to_owned).map_err(js_error)
    }

    #[wasm_bindgen(js_name = snapshotJson)]
    pub fn snapshot_json(&self, revision: &str, layout_revision: &str, offset: usize,
        limit: usize, glyphs: bool) -> Result<String, JsValue>
    {
        self.inner.snapshot_json(id(revision)?, id(layout_revision)?, offset, limit, glyphs).map_err(js_error)
    }

    #[wasm_bindgen(js_name = readingJson)]
    pub fn reading_json(&self, revision: &str, layout_revision: &str, offset: usize,
        limit: usize) -> Result<String, JsValue>
    {
        self.inner.reading_json(id(revision)?, id(layout_revision)?, offset, limit).map_err(js_error)
    }

    #[wasm_bindgen(js_name = pendingAssetsJson)]
    pub fn pending_assets_json(&self, revision: &str, offset: usize, limit: usize) -> Result<String, JsValue> {
        self.inner.pending_assets_json(id(revision)?, offset, limit).map_err(js_error)
    }

    #[wasm_bindgen(js_name = hitTestJson)]
    pub fn hit_test_json(&self, revision: &str, layout_revision: &str, x: f32, y: f32) -> Result<String, JsValue> {
        self.inner.hit_test_json(id(revision)?, id(layout_revision)?, x, y).map_err(js_error)
    }

    #[wasm_bindgen(js_name = selectItemJson)]
    pub fn select_item_json(&self, revision: &str, layout_revision: &str, index: usize,
        start: usize, end: usize) -> Result<String, JsValue>
    {
        self.inner.select_item_json(id(revision)?, id(layout_revision)?, index, start, end).map_err(js_error)
    }
}
