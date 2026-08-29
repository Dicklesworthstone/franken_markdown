import initWasm, {
  capabilities as wasmCapabilities,
  renderHtmlConfiguredAdvanced,
  renderPdfConfiguredMulti
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
  const pdfImages = pdfImagesOption(options.pdfImages);
  const fontAssets = fontAssetsOption(options.fontAssets);
  const fontScale = fontScaleOption(options.fontScale ?? options.typeSize);
  const destinations = pdfImages.map((image) => image.destination);
  const lengths = new Uint32Array(pdfImages.map((image) => image.bytes.length));
  const totalBytes = pdfImages.reduce((sum, image) => sum + image.bytes.length, 0);
  const flatBytes = new Uint8Array(totalBytes);
  let offset = 0;
  for (const image of pdfImages) {
    flatBytes.set(image.bytes, offset);
    offset += image.bytes.length;
  }
  return normalizeResult(
    renderHtmlConfiguredAdvanced(
      String(markdown),
      stringOption(options.font),
      darkModeOption(options.darkMode),
      verbatimOption(options.title),
      verbatimOption(options.customCss),
      Boolean(options.allowRawHtml),
      fontScale,
      fontBytesForSlot(fontAssets, "body-regular"),
      fontBytesForSlot(fontAssets, "body-bold"),
      fontBytesForSlot(fontAssets, "body-italic"),
      fontBytesForSlot(fontAssets, "body-bold-italic"),
      fontBytesForSlot(fontAssets, "mono-regular"),
      fontWeightsForSlots(fontAssets),
      destinations,
      flatBytes,
      lengths
    )
  );
}

export async function renderPdf(markdown, options = {}) {
  await init();
  const pdfImages = pdfImagesOption(options.pdfImages);
  const fontAssets = fontAssetsOption(options.fontAssets);
  const fontScale = fontScaleOption(options.fontScale ?? options.typeSize);
  const baseFontSize = numberOption(options.baseFontSize) ?? (fontScale !== undefined ? 11 * fontScale : undefined);

  // Flatten any number of images into the three parallel arrays the core ABI
  // accepts (wasm-bindgen cannot pass a Vec<Vec<u8>>): a destination per image,
  // all image bytes concatenated, and each image's byte length.
  const destinations = pdfImages.map((image) => image.destination);
  const lengths = new Uint32Array(pdfImages.map((image) => image.bytes.length));
  const totalBytes = pdfImages.reduce((sum, image) => sum + image.bytes.length, 0);
  const flatBytes = new Uint8Array(totalBytes);
  let offset = 0;
  for (const image of pdfImages) {
    flatBytes.set(image.bytes, offset);
    offset += image.bytes.length;
  }

  return normalizeResult(
    renderPdfConfiguredMulti(
      String(markdown),
      stringOption(options.font),
      darkModeOption(options.darkMode),
      verbatimOption(options.title),
      verbatimOption(options.author),
      epochOption(options.metadataEpochSeconds),
      Boolean(options.allowRawHtml),
      Boolean(options.codeLineNumbers),
      destinations,
      flatBytes,
      lengths,
      fontBytesForSlot(fontAssets, "body-regular"),
      fontBytesForSlot(fontAssets, "body-bold"),
      fontBytesForSlot(fontAssets, "body-italic"),
      fontBytesForSlot(fontAssets, "body-bold-italic"),
      fontBytesForSlot(fontAssets, "mono-regular"),
      fontWeightsForSlots(fontAssets),
      baseFontSize,
      numberOption(options.headingScale),
      numberOption(options.tableFontSize),
      Boolean(options.pageNumbers)
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

// Preserve a caller value VERBATIM (including surrounding whitespace), mapping
// only null/undefined/"" to `undefined`. Mirrors the Rust ABI's
// `nonempty_verbatim` so title/author/customCss reach the renderer byte-for-byte
// identical to the native CLI (native never trims them). `stringOption` (which
// trims) is kept only for the enum-like `font` value.
function verbatimOption(value) {
  if (value === undefined || value === null) {
    return undefined;
  }
  const text = String(value);
  return text === "" ? undefined : text;
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


/**
 * Coerce an optional typography override to a finite number for the core
 * ABI. Non-finite values are rejected here so the Rust-side deterministic
 * clamps only ever see well-formed input.
 */
function numberOption(value) {
  if (value === undefined || value === null) {
    return undefined;
  }
  if (typeof value !== "number" || !Number.isFinite(value)) {
    throw new TypeError("typography overrides must be finite numbers");
  }
  return value;
}

function fontScaleOption(value) {
  if (value === undefined || value === null) {
    return undefined;
  }
  if (typeof value === "number") {
    if (!Number.isFinite(value) || value <= 0) {
      throw new TypeError("fontScale must be a positive finite number");
    }
    return Math.min(3.0, Math.max(0.5, value));
  }
  if (typeof value === "string") {
    const trimmed = value.trim().toLowerCase();
    switch (trimmed) {
      case "xs":
      case "x-small":
      case "extra-small":
      case "extrasmall":
      case "tiny":
        return 0.75;
      case "sm":
      case "small":
      case "compact":
        return 0.875;
      case "md":
      case "medium":
      case "normal":
      case "default":
      case "regular":
      case "standard":
        return 1.0;
      case "lg":
      case "large":
      case "comfortable":
        return 1.125;
      case "xl":
      case "x-large":
      case "extra-large":
      case "extralarge":
        return 1.25;
      case "2xl":
      case "xxl":
      case "huge":
      case "display":
        return 1.5;
      default:
        break;
    }
    if (trimmed.endsWith("%")) {
      const parsed = parseFloat(trimmed.slice(0, -1));
      if (Number.isFinite(parsed) && parsed > 0) {
        return Math.min(3.0, Math.max(0.5, parsed / 100));
      }
    }
    if (trimmed.endsWith("rem")) {
      const parsed = parseFloat(trimmed.slice(0, -3));
      if (Number.isFinite(parsed) && parsed > 0) {
        return Math.min(3.0, Math.max(0.5, parsed));
      }
    }
    if (trimmed.endsWith("em")) {
      const parsed = parseFloat(trimmed.slice(0, -2));
      if (Number.isFinite(parsed) && parsed > 0) {
        return Math.min(3.0, Math.max(0.5, parsed));
      }
    }
    if (trimmed.endsWith("px")) {
      const parsed = parseFloat(trimmed.slice(0, -2));
      if (Number.isFinite(parsed) && parsed > 0) {
        return Math.min(3.0, Math.max(0.5, parsed / 16));
      }
    }
    if (trimmed.endsWith("pt")) {
      const parsed = parseFloat(trimmed.slice(0, -2));
      if (Number.isFinite(parsed) && parsed > 0) {
        return Math.min(3.0, Math.max(0.5, parsed / 11));
      }
    }
    const parsed = parseFloat(trimmed);
    if (Number.isFinite(parsed) && parsed > 0) {
      return Math.min(3.0, Math.max(0.5, parsed));
    }
    throw new TypeError(`unknown fontScale '${value}'. Valid choices: xs, sm, md, lg, xl, 2xl, or a number/percentage.`);
  }
  throw new TypeError("fontScale must be a number or string");
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
    const weight = fontWeightOption(asset.weight, `fontAssets[${index}].weight`);
    return Object.freeze({ slot, bytes, weight });
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

function fontWeightsForSlots(assets) {
  const slots = [
    "body-regular",
    "body-bold",
    "body-italic",
    "body-bold-italic",
    "mono-regular"
  ];
  return Uint32Array.from(slots, (slot) => {
    const asset = assets.find((entry) => entry.slot === slot);
    return asset === undefined || asset.weight === undefined ? 0 : asset.weight;
  });
}

function fontWeightOption(value, label) {
  if (value === undefined || value === null) {
    return undefined;
  }
  if (typeof value !== "number" || !Number.isInteger(value) || value < 1 || value > 1000) {
    throw new TypeError(`${label} must be an integer 1..=1000`);
  }
  return value;
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
