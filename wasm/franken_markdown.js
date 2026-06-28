import initWasm, {
  capabilities as wasmCapabilities,
  renderHtmlConfigured,
  renderHtmlConfiguredWithFonts,
  renderPdfConfiguredWithAssets,
  renderPdfConfigured,
  renderPdfConfiguredWithImage
} from "./pkg/franken_markdown.js";

let initPromise = null;

export async function init(input) {
  if (initPromise === null) {
    initPromise = input === undefined ? initWasm() : initWasm({ module_or_path: input });
  }
  try {
    await initPromise;
  } catch (error) {
    initPromise = null;
    throw error;
  }
}

export async function capabilities() {
  await init();
  return parseJson(wasmCapabilities(), "capabilities JSON");
}

export async function renderHtml(markdown, options = {}) {
  await init();
  const fontAssets = fontAssetsOption(options.fontAssets);
  if (fontAssets.length > 0) {
    return normalizeResult(
      renderHtmlConfiguredWithFonts(
        String(markdown),
        stringOption(options.font),
        darkModeOption(options.darkMode),
        stringOption(options.title),
        cssOption(options.customCss),
        Boolean(options.allowRawHtml),
        fontBytesForSlot(fontAssets, "body-regular"),
        fontBytesForSlot(fontAssets, "body-bold"),
        fontBytesForSlot(fontAssets, "body-italic"),
        fontBytesForSlot(fontAssets, "body-bold-italic"),
        fontBytesForSlot(fontAssets, "mono-regular")
      )
    );
  }
  return normalizeResult(
    renderHtmlConfigured(
      String(markdown),
      stringOption(options.font),
      darkModeOption(options.darkMode),
      stringOption(options.title),
      cssOption(options.customCss),
      Boolean(options.allowRawHtml)
    )
  );
}

export async function renderPdf(markdown, options = {}) {
  await init();
  const pdfImages = pdfImagesOption(options.pdfImages);
  const fontAssets = fontAssetsOption(options.fontAssets);
  if (pdfImages.length > 1) {
    throw new TypeError("pdfImages currently accepts at most one image asset");
  }
  if (fontAssets.length > 0) {
    const image = pdfImages[0] ?? { destination: "", bytes: new Uint8Array() };
    return normalizeResult(
      renderPdfConfiguredWithAssets(
        String(markdown),
        stringOption(options.font),
        darkModeOption(options.darkMode),
        stringOption(options.title),
        stringOption(options.author),
        epochOption(options.metadataEpochSeconds),
        Boolean(options.allowRawHtml),
        Boolean(options.codeLineNumbers),
        image.destination,
        image.bytes,
        fontBytesForSlot(fontAssets, "body-regular"),
        fontBytesForSlot(fontAssets, "body-bold"),
        fontBytesForSlot(fontAssets, "body-italic"),
        fontBytesForSlot(fontAssets, "body-bold-italic"),
        fontBytesForSlot(fontAssets, "mono-regular")
      )
    );
  }
  if (pdfImages.length === 1) {
    const image = pdfImages[0];
    return normalizeResult(
      renderPdfConfiguredWithImage(
        String(markdown),
        stringOption(options.font),
        darkModeOption(options.darkMode),
        stringOption(options.title),
        stringOption(options.author),
        epochOption(options.metadataEpochSeconds),
        Boolean(options.allowRawHtml),
        Boolean(options.codeLineNumbers),
        image.destination,
        image.bytes
      )
    );
  }
  return normalizeResult(
    renderPdfConfigured(
      String(markdown),
      stringOption(options.font),
      darkModeOption(options.darkMode),
      stringOption(options.title),
      stringOption(options.author),
      epochOption(options.metadataEpochSeconds),
      Boolean(options.allowRawHtml),
      Boolean(options.codeLineNumbers)
    )
  );
}

export async function createRenderer(input) {
  await init(input);
  return Object.freeze({
    capabilities,
    renderHtml,
    renderPdf
  });
}

function normalizeResult(result) {
  let bytes;
  let diagnostics;
  let format;
  let mimeType;
  let extension;
  let sourceLength;
  try {
    bytes = result.bytes;
    diagnostics = parseDiagnostics(result.diagnosticsJson());
    format = result.format;
    mimeType = result.mimeType;
    extension = result.extension;
    sourceLength = result.sourceLength;
  } finally {
    if (typeof result.free === "function") {
      result.free();
    }
  }
  const output = {
    format,
    mimeType,
    extension,
    sourceLength,
    bytes,
    diagnostics,
    text() {
      return new TextDecoder().decode(bytes);
    },
    blob() {
      if (typeof Blob === "undefined") {
        throw new Error("Blob is not available in this JavaScript runtime");
      }
      return new Blob([bytes], { type: output.mimeType });
    },
    filename(baseName = "document") {
      const cleanBase = String(baseName).trim() || "document";
      return `${cleanBase}.${output.extension}`;
    }
  };
  return Object.freeze(output);
}

function parseDiagnostics(json) {
  if (json === "") {
    return [];
  }
  return parseJson(json, "diagnostics JSON");
}

function parseJson(json, label) {
  try {
    return JSON.parse(json);
  } catch (error) {
    throw new Error(`Invalid ${label} returned by franken_markdown wasm core: ${error.message}`);
  }
}

function stringOption(value) {
  if (value === undefined || value === null) {
    return undefined;
  }
  const text = String(value).trim();
  return text === "" ? undefined : text;
}

function cssOption(value) {
  if (value === undefined || value === null) {
    return undefined;
  }
  const text = String(value);
  return text.trim() === "" ? undefined : text;
}

function darkModeOption(value) {
  if (value === undefined || value === null) {
    return undefined;
  }
  const text = String(value).trim().toLowerCase();
  if (text === "" || text === "auto" || text === "system") {
    return text === "system" ? "auto" : text || undefined;
  }
  if (text === "disabled" || text === "disable" || text === "off" || text === "light") {
    return "disabled";
  }
  throw new TypeError("darkMode must be 'auto' or 'disabled'");
}

function epochOption(value) {
  if (value === undefined || value === null) {
    return undefined;
  }
  const epoch = value;
  if (typeof epoch !== "number") {
    throw new TypeError("metadataEpochSeconds must be a number");
  }
  if (!Number.isSafeInteger(epoch) || epoch < 0) {
    throw new TypeError(
      "metadataEpochSeconds must be a finite non-negative integer <= Number.MAX_SAFE_INTEGER"
    );
  }
  return epoch;
}

function pdfImagesOption(value) {
  if (value === undefined || value === null) {
    return [];
  }
  if (!Array.isArray(value)) {
    throw new TypeError("pdfImages must be an array of { destination, bytes } objects");
  }
  return value.map((asset, index) => {
    if (asset === null || typeof asset !== "object") {
      throw new TypeError(`pdfImages[${index}] must be an object`);
    }
    const destination = stringOption(asset.destination);
    if (destination === undefined) {
      throw new TypeError(`pdfImages[${index}].destination must be a non-empty string`);
    }
    const bytes = bytesOption(asset.bytes, `pdfImages[${index}].bytes`);
    return Object.freeze({ destination, bytes });
  });
}

function fontAssetsOption(value) {
  if (value === undefined || value === null) {
    return [];
  }
  if (!Array.isArray(value)) {
    throw new TypeError("fontAssets must be an array of { slot, bytes } objects");
  }
  const seen = new Set();
  return value.map((asset, index) => {
    if (asset === null || typeof asset !== "object") {
      throw new TypeError(`fontAssets[${index}] must be an object`);
    }
    const slot = fontSlotOption(asset.slot, `fontAssets[${index}].slot`);
    if (seen.has(slot)) {
      throw new TypeError(`fontAssets contains duplicate slot ${slot}`);
    }
    seen.add(slot);
    const bytes = bytesOption(asset.bytes, `fontAssets[${index}].bytes`);
    if (bytes.byteLength === 0) {
      throw new TypeError(`fontAssets[${index}].bytes must not be empty`);
    }
    return Object.freeze({ slot, bytes });
  });
}

function fontSlotOption(value, label) {
  const slot = stringOption(value);
  const allowed = new Set([
    "body-regular",
    "body-bold",
    "body-italic",
    "body-bold-italic",
    "mono-regular"
  ]);
  if (slot === undefined || !allowed.has(slot)) {
    throw new TypeError(
      `${label} must be one of body-regular, body-bold, body-italic, body-bold-italic, mono-regular`
    );
  }
  return slot;
}

function fontBytesForSlot(assets, slot) {
  const asset = assets.find((entry) => entry.slot === slot);
  return asset === undefined ? new Uint8Array() : asset.bytes;
}

function bytesOption(value, label) {
  if (value instanceof Uint8Array) {
    return value;
  }
  if (value instanceof ArrayBuffer) {
    return new Uint8Array(value);
  }
  if (ArrayBuffer.isView(value)) {
    return new Uint8Array(value.buffer, value.byteOffset, value.byteLength);
  }
  throw new TypeError(`${label} must be a Uint8Array, ArrayBuffer, or typed-array view`);
}
