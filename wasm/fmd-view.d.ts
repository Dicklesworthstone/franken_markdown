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
  "fmd-rendered": CustomEvent<FmdRenderedDetail>;
  "fmd-error": CustomEvent<{ error: string }>;
}

export interface FmdView extends HTMLElement {
  addEventListener<K extends keyof FmdViewEventMap>(
    type: K,
    listener: (this: FmdView, ev: FmdViewEventMap[K]) => void,
    options?: boolean | AddEventListenerOptions
  ): void;
  addEventListener(
    type: string,
    listener: EventListenerOrEventListenerObject,
    options?: boolean | AddEventListenerOptions
  ): void;
}

/**
 * Register `<fmd-view>` (idempotent). Auto-registered in browser globals on
 * import; call this explicitly for a non-global registry.
 */
export function registerFmdView(
  registry?: Pick<CustomElementRegistry, "define" | "get">
): "fmd-view";
