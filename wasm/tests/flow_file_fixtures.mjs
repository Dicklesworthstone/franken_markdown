// Explicit file-system double. Native File/Blob and UTF-8 codecs are real;
// permission dialogs, OS locking and atomic replacement are NOT modeled proof.
export const deferred = () => {
  let resolve, reject;
  const promise = new Promise((yes, no) => {
    resolve = yes;
    reject = no;
  });
  return { promise, resolve, reject };
};
export class MemoryFileHandle {
  kind = "file";
  permission = "granted";
  log = [];
  hooks = {};
  counts = {};
  constructor(name = "notes.md", source = "# Disk\n", entry) {
    this.name = name;
    this.entry = entry ?? { bytes: new TextEncoder().encode(source) };
  }
  set source(value) {
    this.entry.bytes = new TextEncoder().encode(value);
  }
  get source() {
    return new TextDecoder("utf-8", { fatal: true, ignoreBOM: true }).decode(this.entry.bytes);
  }
  async stage(name, value) {
    this.log.push(name);
    this.counts[name] = (this.counts[name] ?? 0) + 1;
    return this.hooks[name]?.(this.counts[name], value);
  }
  async getFile() {
    await this.stage("read");
    return new File([this.entry.bytes], this.name, { lastModified: 1, type: "text/markdown" });
  }
  async isSameEntry(other) {
    await this.stage("identity");
    return this.entry === other.entry;
  }
  async requestPermission(options) {
    await this.stage("permission", options);
    return this.permission;
  }
  async createWritable(options) {
    await this.stage("create", options);
    let staged = null,
      aborted = false;
    return {
      write: async (blob) => {
        await this.stage("write", blob);
        if (aborted) throw new DOMException("cancelled", "AbortError");
        staged = new Uint8Array(await blob.arrayBuffer());
      },
      close: async () => {
        await this.stage("close");
        if (aborted) throw new DOMException("cancelled", "AbortError");
        this.entry.bytes = staged;
        await this.stage("committed");
      },
      abort: async () => {
        aborted = true;
        await this.stage("abort");
      },
    };
  }
}
