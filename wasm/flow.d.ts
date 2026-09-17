/** u64 wire values are lossless decimal strings. Number is never an identity. */
export type FlowIdentity = string | bigint;
export interface FlowToken { readonly revision: string; readonly layoutRevision: string; }
export interface FlowTokenInput { readonly revision: FlowIdentity; readonly layoutRevision: FlowIdentity; }
export interface FlowRect { readonly x: number; readonly y: number; readonly width: number; readonly height: number; }
/** Enclosing original Markdown bytes, not an exact inline selection map. */
export interface FlowSourceSpan { readonly startByte: number; readonly endByte: number; }
export interface FlowLayoutOptions {
  viewportWidth?: number;
  bodySize?: number;
  codeSize?: number;
  lineHeight?: number;
}
export interface FlowCreateOptions extends FlowLayoutOptions { font?: "sans" | "serif"; }
export interface FlowEditOptions {
  expectedRevision: FlowIdentity;
  /** Explicitly attest external bytes, base URI and authorization are unchanged. Defaults false. */
  reuseAssets?: boolean;
}
export interface FlowPageOptions { offset?: number; limit?: number; token?: FlowTokenInput; }
export interface FlowSnapshotOptions extends FlowPageOptions { glyphs?: boolean; }
export interface FlowPage extends FlowToken {
  readonly schemaVersion: 1;
  readonly offset: number;
  readonly total: number;
  readonly nextOffset: number | null;
}
export interface FlowGlyph {
  readonly glyphId: number;
  readonly clusterIndex: number;
  readonly fontId: string;
  readonly xAdvance: number;
  readonly yAdvance: number;
  readonly xOffset: number;
  readonly yOffset: number;
}
export interface FlowCluster {
  readonly index: number;
  readonly bytes: readonly [number, number];
  readonly utf16: readonly [number, number];
  readonly glyphs: readonly [number, number];
  readonly xStart: number;
  readonly xEnd: number;
  readonly fontId: string;
}
export interface FlowFontRun {
  readonly coordinateSpace: "fragment-utf8-and-utf16";
  readonly fontId: string;
  readonly fontSize: number;
  readonly direction: "ltr" | "rtl";
  readonly fontOrigin: "bundled" | "system";
  readonly script: string;
  readonly language: string;
  readonly totalAdvance: number;
  readonly clusterCount: number;
  readonly glyphCount: number;
  readonly clusters?: readonly FlowCluster[];
  readonly glyphs?: readonly FlowGlyph[];
}
export interface FlowItemBase {
  readonly index: number;
  readonly bounds: FlowRect;
  readonly enclosingSourceSpan: FlowSourceSpan;
}
export interface FlowTextItem extends FlowItemBase {
  readonly kind: "text";
  readonly text: string;
  readonly colorRole: string;
  readonly fontSize: number;
  readonly fontRun: FlowFontRun | null;
}
export interface FlowImageItem extends FlowItemBase {
  readonly kind: "image";
  readonly requestId: string;
  readonly destination: string;
  readonly altText: string;
  readonly isResolved: boolean;
}
export interface FlowAnchorItem extends FlowItemBase {
  readonly kind: "anchor";
  readonly target: string;
  readonly isHeading: boolean;
  readonly level: number;
}
export interface FlowVectorItem extends FlowItemBase {
  readonly kind: "vector";
  readonly shape: "horizontal-rule" | "table-border" | "callout-accent-bar" | "checkbox-outline"
    | "checkbox-check" | "diagram-box" | "diagram-arrow" | "diagram-connector";
  readonly strokeWidth: number;
  readonly colorRole: string;
}
export interface FlowClipItem extends FlowItemBase { readonly kind: "clip"; readonly childCount: number; }
export type FlowItem = FlowTextItem | FlowImageItem | FlowAnchorItem | FlowVectorItem | FlowClipItem;
export interface FlowSnapshot extends FlowPage {
  readonly sourceLengthBytes: number;
  readonly sourceLengthUtf16: number;
  readonly pendingAssetCount: number;
  readonly readingNodeCount: number;
  readonly shapingProfile: "bundled-simple-ltr";
  readonly sourceMapping: "enclosing-block-bytes";
  readonly totalBounds: FlowRect;
  readonly options: Readonly<Required<FlowLayoutOptions>>;
  readonly items: readonly FlowItem[];
  readonly cache: {
    readonly hits: string;
    readonly misses: string;
    readonly evictions: string;
    readonly retainedEntries: number;
    readonly retainedPayloadBytes: number;
  };
}
export interface FlowReadingNode {
  readonly role: "document" | "heading" | "paragraph" | "code-block" | "list" | "list-item"
    | "table" | "table-header-row" | "table-row" | "table-header-cell" | "table-cell"
    | "blockquote" | "thematic-break" | "image";
  readonly level?: number;
  readonly text: string;
  readonly bounds: FlowRect;
  readonly enclosingSourceSpan: FlowSourceSpan;
  readonly children: readonly FlowReadingNode[];
}
export interface FlowReadingPage extends FlowPage { readonly nodes: readonly FlowReadingNode[]; }
export interface FlowAssetRequest {
  readonly id: string;
  readonly generation: string;
  readonly kind: string;
  readonly url: string;
  readonly altText: string;
  readonly enclosingSourceByteOffset: number;
  readonly estimatedWidth: number;
  readonly estimatedHeight: number;
}
export interface FlowAssetPage extends FlowPage { readonly requests: readonly FlowAssetRequest[]; }
export interface FlowAssetResult {
  requestId: FlowIdentity;
  generation: FlowIdentity;
  width: number;
  height: number;
  /** Omit for a dimension-only result. Payloads are never fetched by the engine. */
  bytes?: Uint8Array;
}
export interface FlowHit extends FlowToken {
  readonly schemaVersion: 1;
  readonly linkTarget: string | null;
  readonly hit: null | {
    readonly itemIndex: number;
    readonly enclosingSourceSpan: FlowSourceSpan;
    /** Present for shaped text, absent for an image hit. */
    readonly coordinateSpace?: "fragment-utf16";
    readonly byteOffset?: number;
    readonly utf16Offset?: number;
    readonly clusterIndex?: number;
    readonly isExact?: boolean;
    readonly visualX?: number;
    readonly affinity?: "leading" | "trailing";
  };
}
export interface FlowSelection extends FlowToken {
  readonly schemaVersion: 1;
  readonly itemIndex: number;
  readonly coordinateSpace: "fragment-utf16";
  readonly text: string;
  readonly enclosingSourceSpan: FlowSourceSpan;
  readonly rectangles: readonly FlowRect[];
}
/** Synchronous after creation. Run in a Worker for off-main-thread editing.
 * Edits still reparse complete bounded snapshots; this is not incremental parsing.
 */
export interface FlowSession {
  readonly disposed: boolean;
  readonly revision: string;
  readonly layoutRevision: string;
  readonly token: FlowToken;
  readonly source: string;
  readonly layoutOptions: Required<FlowLayoutOptions>;
  /** Offsets refer to the original source in UTF-16 units, as textarea uses. */
  edit(startUtf16: number, endUtf16: number, replacement: string, options: FlowEditOptions): FlowToken;
  editBytes(startByte: number, endByte: number, replacement: string, options: FlowEditOptions): FlowToken;
  replaceSource(source: string, options: FlowEditOptions): FlowToken;
  reflow(options: FlowLayoutOptions, token: FlowTokenInput): FlowToken;
  provideAsset(result: FlowAssetResult): FlowToken;
  reloadAssets(expectedRevision: FlowIdentity): FlowToken;
  snapshot(options?: FlowSnapshotOptions): FlowSnapshot;
  readingOrder(options?: FlowPageOptions): FlowReadingPage;
  pendingAssets(options?: FlowPageOptions): FlowAssetPage;
  /** Captures a token when called; editing between pages makes iteration throw. */
  pages(options?: Omit<FlowSnapshotOptions, "offset">): IterableIterator<FlowSnapshot>;
  hitTest(x: number, y: number, token: FlowTokenInput): FlowHit;
  /** Selects fragment-local reading text, not original Markdown offsets. */
  selectText(itemIndex: number, startUtf16: number, endUtf16: number, token: FlowTokenInput): FlowSelection;
  copySource(startByte: number, endByte: number, expectedRevision: FlowIdentity): string;
  fontBytes(fontId: FlowIdentity): Uint8Array;
  /** Exact immutable-font paths. At most 256 glyph IDs; empty requests return metrics only. */
  glyphOutlines(fontId: FlowIdentity, glyphIds: readonly number[] | Uint16Array): FlowGlyphOutlines;
  assetBytes(requestId: FlowIdentity, expectedRevision: FlowIdentity): Uint8Array | null;
  /** Idempotent; releases Rust-owned source, display and font-run cache. */
  dispose(): void;
}
export class FlowError extends Error {
  readonly code: string;
  constructor(code: string, message: string, options?: ErrorOptions);
}
export function init(input?: string | URL | Request | Response | BufferSource | WebAssembly.Module): Promise<void>;
/** Shares the package's initialized WASM instance. Enforces 4 MiB UTF-8 source
 * admission before WASM ingress; rejects unpaired JS surrogates instead of
 * silently replacing source text. No network/image loading is performed.
 */
export function createFlowSession(markdown: string, options?: FlowCreateOptions): Promise<FlowSession>;

/** Baseline-relative y-up font design units, filled with nonzero winding. */
export type FlowPathCommand = readonly ["M", number, number] | readonly ["L", number, number]
  | readonly ["Q", number, number, number, number] | readonly ["Z"];
export interface FlowGlyphOutline { readonly glyphId: number; readonly commands: readonly FlowPathCommand[]; }
export interface FlowGlyphOutlines {
  readonly schemaVersion: 1;
  readonly fontId: string;
  readonly unitsPerEm: number;
  readonly ascent: number;
  readonly descent: number;
  readonly lineGap: number;
  readonly glyphs: readonly FlowGlyphOutline[];
}
