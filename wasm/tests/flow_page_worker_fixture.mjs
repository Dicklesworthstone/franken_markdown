// Real worker and production facades; corrupt replies only after worker-side
// validation, so these fixtures exercise the client's request ownership fence.
import { parentPort, workerData } from "node:worker_threads";
import { createFlowAdapter } from "../flow_session.mjs";
import { installFlowWorker } from "../flow_worker_session.mjs";

const requests = new Map();
const counts = new Map();
const listeners = new Map();
const endpoint = {
  postMessage(message, transfer) {
    const request = requests.get(message.id);
    requests.delete(message.id);
    if (message.ok && request?.method === "create" && workerData.legacy)
      message.value = null;
    const method = request?.method === "snapshotDelta" ? "snapshot" : request?.method;
    if (message.ok && method === (workerData.faultMethod ?? "snapshot")) {
      const count = (counts.get(method) ?? 0) + 1;
      counts.set(method, count);
      if (count === (workerData.faultAt ?? 1)) {
        const page = message.value.page ?? message.value;
        const key = { snapshot: "items", readingOrder: "nodes", pendingAssets: "requests" }[method];
        switch (workerData.fault) {
          case "offset":
            page.offset = 0;
            page.nextOffset = 1;
            break;
          case "short":
            page[key].pop();
            page.nextOffset = page.offset + page[key].length;
            break;
          case "long":
            page[key].push({ ...page[key][0] });
            page.nextOffset = page.offset + page[key].length;
            break;
          case "total":
            page.total = 0x100000000;
            break;
          case "revision":
          case "layoutRevision": {
            const field = workerData.fault;
            page[field] = String(BigInt(page[field]) + 1n);
            message.state = {
              ...message.state,
              token: { ...message.state.token, [field]: page[field] },
            };
            break;
          }
        }
      }
    }
    parentPort.postMessage(message, transfer);
  },
  addEventListener(kind, listener) {
    const wrapped = (data) => {
      requests.set(data.id, data);
      listener({ data });
    };
    listeners.set(listener, wrapped);
    parentPort.on(kind, wrapped);
  },
  removeEventListener(kind, listener) {
    parentPort.off(kind, listeners.get(listener));
    listeners.delete(listener);
  },
};

function bad(code) {
  throw JSON.stringify({ code, message: `synthetic ${code}` });
}
class Raw {
  revision = "1";
  layoutRevision = "1";
  #source;
  constructor(source) {
    this.#source = source;
  }
  get source() {
    return this.#source;
  }
  replaceSource(revision, source) {
    if (revision !== this.revision) bad("STALE_REVISION");
    this.#source = source;
    this.revision = String(BigInt(this.revision) + 1n);
    this.layoutRevision = String(BigInt(this.layoutRevision) + 1n);
  }
  head() {
    return { schemaVersion: 1, revision: this.revision, layoutRevision: this.layoutRevision };
  }
  page(key, offset, limit) {
    const all = (this.#source === "" ? [] : this.#source.split("\n")).map((text, index) =>
      key === "requests"
        ? { id: String(index + 1), generation: this.revision, url: `image-${index}.png` }
        : { index, kind: "text", text }
    );
    // Native browser::page rejects beyond-end offsets but permits offset == len.
    if (offset > all.length) bad("INVALID_PAGE");
    const list = all.slice(offset, offset + limit);
    return JSON.stringify({
      ...this.head(),
      offset,
      total: all.length,
      nextOffset: offset + list.length < all.length ? offset + list.length : null,
      [key]: list,
    });
  }
  snapshotJson(_revision, _layout, offset, limit) {
    return this.page("items", offset, limit);
  }
  readingJson(_revision, _layout, offset, limit) {
    return this.page("nodes", offset, limit);
  }
  pendingAssetsJson(_revision, offset, limit) {
    return this.page("requests", offset, limit);
  }
  hitTestJson() {
    return JSON.stringify({ ...this.head(), linkTarget: null, hit: null });
  }
  selectItemJson(_revision, _layout, index, start, end) {
    return JSON.stringify({ ...this.head(), text: this.#source.split("\n")[index].slice(start, end) });
  }
  free() {}
}
installFlowWorker(endpoint, (source, options) => {
  const { font, ...layout } = options;
  return createFlowAdapter(new Raw(source), layout);
});
