import type {
  FlowPageOptions,
  FlowReadingInlineRun,
  FlowReadingLink,
  FlowReadingListItem,
  FlowReadingNode,
  FlowReadingPage,
  FlowRect,
  FlowSourceSpan,
  FlowToken,
  FlowTokenInput,
} from "./flow.js";
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
  /** 50,000 runs across all pages and descendants. */
  maxInlineRuns?: number;
  /** 1,048,576 UTF-16 units across retained and active link targets. */
  maxLinkUnits?: number;
  /** 50,000 ancestry entries, counting repeated paths across all roots. */
  maxListEntries?: number;
  /** 1,048,576 UTF-16 units across retained heading/note destination IDs. */
  maxAnchorUnits?: number;
}
export interface FlowReadOptions {
  token?: FlowTokenInput;
  /** Stops waiting only; never forwarded to a worker RPC. */
  signal?: AbortSignal;
  limits?: FlowReadingLimits;
}
export interface FlowReadingInlineEntry extends FlowReadingInlineRun {
  /** Validated UTF-16 coordinates in this node's reading text, not Markdown. */
  readonly startUtf16: number;
  readonly endUtf16: number;
}
export interface FlowReadingEntry
  extends Omit<FlowReadingNode, "children" | "inlineRuns" | "imageLink" | "listPath" | "anchorId"> {
  /** Preorder index, scoped to this exact snapshot, not a persistent node ID. */
  readonly index: number;
  /** Exact engine-assigned destination, null when absent or legacy-unknown. */
  readonly anchorId: string | null;
  readonly children: readonly FlowReadingEntry[];
  readonly inlineRuns: readonly FlowReadingInlineEntry[];
  readonly imageLink: FlowReadingLink | null;
  /** Null for legacy unknown ancestry; empty for known outside-list blocks.
   * Present paths are immutable and validated across the complete snapshot. */
  readonly listPath: readonly FlowReadingListItem[] | null;
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
  /** Default false. Frozen Unicode 15.1 default full case folding, non-Turkic.
   * Mutually exclusive with asciiCaseInsensitive:true. No normalization or
   * accent removal. Expansions must match whole source scalars (ss finds ß;
   * s does not match half of ß). Original selectable UTF-16 ranges are retained. */
  caseInsensitive?: boolean;
  /** Default false. Adjacent Unicode letters/marks/numbers/connectors/joiners
   * prevent a match. Not locale-sensitive or UAX #29 word segmentation. */
  wholeWord?: boolean;
  /** Default and maximum 1,000. Results report whether additional matches exist. */
  maxMatches?: number;
}
export interface FlowReadingAsyncSearchOptions extends FlowReadingSearchOptions {
  /** Cancels local cooperative search; never sends a worker RPC or terminates
   * the session. Source edits, reflows and disposal invalidate pending work. */
  signal?: AbortSignal;
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
  readonly inlineRunCount: number;
  readonly linkUnits: number;
  readonly listEntryCount: number;
  readonly anchorUnits: number;
  assertCurrent(): void;
  locate(index: number): FlowReadingLocation;
  /** Resolve a local #fragment using engine IDs, percent-decoding once without
   * changing case or treating '+' as space. Null for external, unknown, empty,
   * malformed, or legacy-unavailable targets. Never navigates or performs I/O.
   * Targets must be well-formed Unicode strings of at most 4,096 UTF-16 units;
   * invalid arguments and stale/disposed snapshots throw FlowReadingError. */
  locateFragment(target: string): FlowReadingLocation | null;
  /** Literal, non-overlapping matches within a leaf, not across semantic blocks.
   * Exact Unicode scalar boundaries; offsets remain UTF-16. Query <=1,024 units. */
  find(query: string, options?: FlowReadingSearchOptions): FlowReadingSearchResult;
  /** Same matching and bounds as find, but yields real event-loop turns during
   * large scans. No partial results. Auxiliary scanning memory is query-sized. */
  findAsync(query: string, options?: FlowReadingAsyncSearchOptions): Promise<FlowReadingSearchResult>;
  /** Accepts only genuine matches returned by this snapshot; fences revisions. */
  matchText(match: FlowReadingMatch): string;
}
export function readFlowDocument(
  session: ReadingFlowSession,
  options?: FlowReadOptions,
): Promise<FlowReadingDocument>;
/** Host must fence the source revision first. Validates Unicode and UTF-8 bounds;
 * source is limited to 4 MiB UTF-8, matching the browser flow admission limit. */
export function sourceSpanToUtf16(
  source: string,
  span: FlowSourceSpan,
): Readonly<{ start: number; end: number }>;
export interface FlowReadingLinkActivation {
  /** Passed the conservative scheme filter; the host must still authorize it. */
  readonly target: string;
  readonly link: FlowReadingLink;
  readonly location: FlowReadingLocation;
  /** Logical reading-text range, null for a linked image description. */
  readonly startUtf16: number | null;
  readonly endUtf16: number | null;
}
export interface FlowReaderViewOptions {
  /** Opt-in callback for primary click/Enter; omitted means inert link text.
   * URLs never become href/src attributes. Hosts own navigation, authorization,
   * unsubmitted-editor fences and async error handling. Stale sessions refuse
   * activation. Adjacent style runs in the same link share one Tab stop. */
  onLink?: (activation: FlowReadingLinkActivation) => void;
}
export class FlowReaderView {
  constructor(container: HTMLElement, options?: FlowReaderViewOptions);
  readonly disposed: boolean;
  readonly document: FlowReadingDocument | null;
  /** Builds safe semantic DOM offscreen and replaces only this container's
   * children. Preparation failures retain prior DOM. The same snapshot is a
   * no-op, preserving native focus/selection while the Canvas merely scrolls. */
  render(document: FlowReadingDocument): void;
  /** Focus without automatically scrolling the page or navigating a URL. */
  focusNode(index: number): FlowReadingLocation;
  /** Uses native DOM Range selection across styled text segments. Refuses
   * changed, inserted or reordered DOM text. No clipboard writes/permissions. */
  selectMatch(match: FlowReadingMatch): string;
  clear(): void;
  /** Clears only its DOM. Does not dispose the session or remove the container. */
  dispose(): void;
}
