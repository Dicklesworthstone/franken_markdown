// Explicit MutationObserver event-contract fixture. Callback delivery is manual
// so these Node tests exercise synchronous takeRecords fences independently of
// the real Chromium callback/microtask tests in flow_document_browser.mjs.
export function sourceAccessFixture(fields) {
  const observers = new Set();
  class Observer {
    targets = new Map();
    records = [];
    constructor(callback) {
      this.callback = callback;
      observers.add(this);
    }
    observe(target, options) { this.targets.set(target, options); }
    takeRecords() { return this.records.splice(0); }
    disconnect() {
      this.records.length = 0;
      this.targets.clear();
      observers.delete(this);
    }
  }
  const doc = { defaultView: { MutationObserver: Observer } };
  function record(target, value) {
    for (const observer of observers) {
      const options = observer.targets.get(target);
      if (value.type === "attributes" ? options?.attributeFilter?.includes(value.attributeName)
        : options?.childList) observer.records.push({ target, ...value });
    }
  }
  for (const field of fields) {
    field.ownerDocument = doc;
    field.isConnected = true;
    for (const [property, attributeName] of [["readOnly", "readonly"], ["disabled", "disabled"]]) {
      let value = !!field[property];
      Object.defineProperty(field, property, {
        configurable: true,
        get: () => value,
        set(next) {
          value = !!next;
          record(field, { type: "attributes", attributeName });
        },
      });
    }
  }
  return {
    get observerCount() { return observers.size; },
    get pendingRecords() { return [...observers].reduce((sum, owner) => sum + owner.records.length, 0); },
    deliver() {
      for (const observer of observers) {
        const records = observer.takeRecords();
        if (records.length) observer.callback(records);
      }
    },
    mount(field, connected) {
      field.isConnected = connected;
      record(doc, { type: "childList", addedNodes: connected ? [field] : [], removedNodes: connected ? [] : [field] });
    },
    unrelated() {
      record(doc, { type: "childList", addedNodes: [{}], removedNodes: [] });
    },
  };
}
