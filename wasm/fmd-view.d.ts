/**
 * `<fmd-view>` web component type surface (bead uito).
 */

export interface FmdRenderedDetail {
  bytes: number;
  sourceLength: number;
  diagnostics: Array<{
    severity: "warning" | "error";
    start: number;
    end: number;
    message: string;
  }>;
}

export interface FmdViewEventMap {
  /** Emitted by the current connected render after embedded fonts decode and HTML is published. */
  "fmd-rendered": CustomEvent<FmdRenderedDetail>;
  /** Current fetch, initialization, render, font or output failure; canceled work is silent. */
  "fmd-error": CustomEvent<{ error: string }>;
}

/**
 * Reactive source: a nonempty `src` attribute, otherwise the first
 * `script[type="text/markdown"]` child descendant or exact inline text.
 * Inline mutations and `src`/`font`/`dark-mode` changes render the latest
 * snapshot. Superseded fetches and disconnection cancel publication; reconnect
 * renders current content. Failed engine initialization can be retried by a
 * subsequent source/options change or reconnect.
 * The document theme is scoped to the shadow host. Embedded body/mono faces
 * have per-render family names; replacing or disconnecting a view retires only
 * its registrations. A failed render/font load retains the previous view.
 */
export interface FmdView extends HTMLElement {
  addEventListener<K extends keyof FmdViewEventMap>(
    type: K,
    listener: (this: FmdView, ev: FmdViewEventMap[K]) => void,
    options?: boolean | AddEventListenerOptions,
  ): void;
  addEventListener(
    type: string,
    listener: EventListenerOrEventListenerObject,
    options?: boolean | AddEventListenerOptions,
  ): void;
}

/**
 * Register `<fmd-view>` (idempotent). Auto-registered in browser globals on
 * import; call this explicitly for a non-global registry.
 */
export function registerFmdView(
  registry?: Pick<CustomElementRegistry, "define" | "get">,
): "fmd-view";
