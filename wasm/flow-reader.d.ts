import type { FlowPageOptions, FlowReadingNode, FlowReadingPage, FlowRect, FlowSourceSpan, FlowToken, FlowTokenInput } from "./flow.js";
export interface ReadingFlowSession {
  readonly disposed: boolean;
  readonly token: FlowToken;
  readingOrder(options?: FlowPageOptions): FlowReadingPage | Promise<FlowReadingPage>;
}
export interface FlowReadingLimits {
  /** 10,000 roots AND descendants combined. Defaults may only be lowered. */
  maxNodes?: number;
  /** 64 levels below a root. */
  maxDepth?: number;
  /** 1,048,576 UTF-16 units, including container transcripts. */
  maxTextUnits?: number;
}
export interface FlowReadOptions {
  token?: FlowTokenInput;
  /** Stops waiting only; never forwarded to a worker RPC. */
  signal?: AbortSignal;
  limits?: FlowReadingLimits;
}
export interface FlowReadingEntry extends Omit<FlowReadingNode, "children"> {
  /** Preorder index, scoped to this exact snapshot, not a persistent node ID. */
  readonly index: number;
  readonly children: readonly FlowReadingEntry[];
}
export interface FlowReadingLocation extends FlowToken {
  readonly nodeIndex: number;
  readonly bounds: FlowRect;
  /** Enclosing Markdown block bytes, never an exact inline selection map. */
  readonly enclosingSourceSpan: FlowSourceSpan;
}
export interface FlowReadingMatch extends FlowToken {
  readonly nodeIndex: number;
  /** Offsets in this reading node's unsplit logical text, not source or glyphs. */
  readonly startUtf16: number;
  readonly endUtf16: number;
}
export interface FlowReadingSearchOptions {
  /** Default false. Folds A-Z only; no locale, normalization or Unicode folding. */
  asciiCaseInsensitive?: boolean;
  /** Default and maximum 1,000. Results report whether additional matches exist. */
  maxMatches?: number;
}
export interface FlowReadingSearchResult {
  readonly matches: readonly FlowReadingMatch[];
  readonly truncated: boolean;
}
export class FlowReadingError extends Error {
  readonly code: string;
  constructor(code: string, message: string);
}
export class FlowReadingDocument {
  private constructor();
  readonly token: FlowToken;
  readonly roots: readonly FlowReadingEntry[];
  readonly nodes: readonly FlowReadingEntry[];
  readonly headings: readonly FlowReadingEntry[];
  /** Logical leaves separated by blank lines; aggregate container text omitted. */
  readonly text: string;
  readonly textUnits: number;
  assertCurrent(): void;
  locate(index: number): FlowReadingLocation;
  /** Literal, non-overlapping matches within a leaf, not across semantic blocks.
   * Exact Unicode scalar boundaries; offsets remain UTF-16. Query <=1,024 units. */
  find(query: string, options?: FlowReadingSearchOptions): FlowReadingSearchResult;
  /** Accepts only genuine matches returned by this snapshot; fences revisions. */
  matchText(match: FlowReadingMatch): string;
}
export function readFlowDocument(session: ReadingFlowSession, options?: FlowReadOptions): Promise<FlowReadingDocument>;
/** Host must fence the source revision first. Validates Unicode and UTF-8 bounds;
 * source is limited to 4 MiB UTF-8, matching the browser flow admission limit. */
export function sourceSpanToUtf16(source: string, span: FlowSourceSpan): Readonly<{ start: number; end: number }>;
export class FlowReaderView {
  constructor(container: HTMLElement);
  readonly disposed: boolean;
  readonly document: FlowReadingDocument | null;
  /** Builds safe semantic DOM offscreen and replaces only this container's
   * children. Preparation failures retain prior DOM. The same snapshot is a
   * no-op, preserving native focus/selection while the Canvas merely scrolls. */
  render(document: FlowReadingDocument): void;
  /** Focus without automatically scrolling the page or navigating a URL. */
  focusNode(index: number): FlowReadingLocation;
  /** Uses native DOM Range selection. No clipboard writes or permissions. */
  selectMatch(match: FlowReadingMatch): string;
  clear(): void;
  /** Clears only its DOM. Does not dispose the session or remove the container. */
  dispose(): void;
}
