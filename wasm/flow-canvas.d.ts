import type {
  FlowGlyphOutlines,
  FlowHit,
  FlowIdentity,
  FlowLayoutOptions,
  FlowRect,
  FlowSnapshot,
  FlowSnapshotOptions,
  FlowToken,
  FlowTokenInput,
} from "./flow.js";

/** Structural subset satisfied by both synchronous and worker flow sessions. */
export interface CanvasFlowSession {
  readonly disposed: boolean;
  readonly token: FlowToken;
  readonly layoutOptions: Required<FlowLayoutOptions>;
  snapshot(options?: FlowSnapshotOptions): FlowSnapshot | Promise<FlowSnapshot>;
  glyphOutlines(
    fontId: FlowIdentity,
    glyphIds: readonly number[] | Uint16Array,
  ): FlowGlyphOutlines | Promise<FlowGlyphOutlines>;
  hitTest(x: number, y: number, token: FlowTokenInput): FlowHit | Promise<FlowHit>;
}
/** May lower the defaults, never raise them. Counts bound retained/processed
 * structures, not exact browser allocator overhead or GPU memory. */
export interface FlowCanvasLimits {
  maxPixels?: number; // 16,777,216 pixels per surface; staging and target coexist.
  maxScannedItems?: number; // 500,000: the complete inventory is still scanned.
  maxVisibleItems?: number; // 10,000
  maxGlyphs?: number; // 50,000
  maxDrawCommands?: number; // 2,000,000 including repeated glyph drawing.
  maxCachedGlyphs?: number; // 2,048
  maxCachedCommands?: number; // 262,144
  maxFrameCommands?: number; // 1,048,576 distinct visible path commands.
}
export type FlowCanvasColor =
  | "background"
  | "text"
  | "heading"
  | "code"
  | "link"
  | "border"
  | "table-border"
  | "accent"
  | "quote"
  | "muted"
  | "strikethrough"
  | "selection";
export interface FlowCanvasOptions {
  /** Must return a fresh surface of exactly the requested pixel size, never
   * the target. Defaults to OffscreenCanvas, then a detached HTML canvas. */
  canvasFactory?: (pixelWidth: number, pixelHeight: number) => HTMLCanvasElement | OffscreenCanvas;
  limits?: FlowCanvasLimits;
  colors?: Partial<Record<FlowCanvasColor, string>>;
}
export interface FlowCanvasImage {
  readonly requestId: string;
  readonly destination: string;
  readonly altText: string;
  readonly isResolved: boolean;
  readonly bounds: FlowRect;
}
export interface FlowCanvasPaintOptions {
  /** Logical viewport size, not layout width. Reflow the session separately. */
  width?: number;
  height?: number;
  scrollX?: number;
  scrollY?: number;
  /** Explicit device-pixel ratio in (0, 8]; default 1, never read from ambient state. */
  pixelRatio?: number;
  token?: FlowTokenInput;
  /** Cancels paint waiting only; never terminates the worker/editing session. */
  signal?: AbortSignal;
  /** Called only for resolved image descriptors. Return an already-authorized,
   * decoded image or null for a placeholder. No fetching/decoding is implicit.
   * The renderer never closes returned caller-owned ImageBitmaps. */
  resolveImage?: (
    image: FlowCanvasImage,
    token: FlowToken,
  ) => CanvasImageSource | null | Promise<CanvasImageSource | null>;
  selection?: FlowTokenInput & { readonly rectangles: readonly FlowRect[] };
}
export interface FlowCanvasFrame extends FlowToken {
  readonly width: number;
  readonly height: number;
  readonly pixelRatio: number;
  readonly scrollX: number;
  readonly scrollY: number;
  readonly totalBounds: FlowRect;
  readonly scannedItems: number;
  readonly visibleItems: number;
  readonly glyphs: number;
  readonly missingImages: number;
}
/** Draw exact bundled glyph paths, not browser-shaped text. The caller owns
 * the session, CSS dimensions, scrolling, accessibility DOM and navigation.
 * Async/preparation failures preserve the prior frame. A browser allocation or
 * final blit failure cannot be rolled back. Dispose/clear revoke shown pixels.
 */
export class FlowCanvasRenderer {
  constructor(canvas: HTMLCanvasElement | OffscreenCanvas, options?: FlowCanvasOptions);
  readonly disposed: boolean;
  readonly frame: FlowCanvasFrame | null;
  readonly cacheStats: Readonly<{ glyphs: number; commands: number }>;
  render(session: CanvasFlowSession, options?: FlowCanvasPaintOptions): Promise<FlowCanvasFrame>;
  /** Local logical viewport coordinates; not client coordinates or device pixels. */
  documentPoint(x: number, y: number): Readonly<{ x: number; y: number }> | null;
  hitTest(x: number, y: number): Promise<FlowHit>;
  clearCache(): void;
  clear(): void;
  /** Idempotent. Releases cached paths and pixels, never disposes the session. */
  dispose(): void;
}
