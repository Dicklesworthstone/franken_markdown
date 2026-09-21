import type {
  FlowReadingDocument, FlowReadingSearchOptions, FlowReadingAsyncSearchOptions,
  FlowReadingSearchResult, FlowReadingMatch,
} from "./flow-reader.js";
import { FlowReaderView } from "./flow-reader.js";

declare const document: FlowReadingDocument;
declare const root: HTMLElement;
const syncOptions: FlowReadingSearchOptions = { caseInsensitive: true, wholeWord: true, maxMatches: 20 };
const asyncOptions: FlowReadingAsyncSearchOptions = { ...syncOptions, signal: new AbortController().signal };
const synchronous: FlowReadingSearchResult = document.find("STRASSE", syncOptions);
const pending: Promise<FlowReadingSearchResult> = document.findAsync("STRASSE", asyncOptions);
const result = await pending;
const match: FlowReadingMatch | undefined = result.matches[0];
const view = new FlowReaderView(root);
view.render(document);
if (match) {
  const original: string = document.matchText(match);
  const selected: string = view.selectMatch(match);
  const offset: number = match.startUtf16;
  void [original, selected, offset];
  // @ts-expect-error matches are immutable snapshot coordinates
  match.endUtf16 = 0;
}
// @ts-expect-error results are immutable
result.matches.push(synchronous.matches[0]);
// @ts-expect-error only async search accepts cancellation
 document.find("x", { signal: new AbortController().signal });
// @ts-expect-error typed boolean, not coercion
 document.findAsync("x", { caseInsensitive: "true" });
// @ts-expect-error no regex query input
 document.findAsync(/x/);
// @ts-expect-error no locale-tailoring option
 document.findAsync("x", { locale: "tr" });
void document.find("A", { asciiCaseInsensitive: true });
void document.findAsync("A", { asciiCaseInsensitive: true });
