// Data-only, engine-independent projects. This factory is also embedded in
// standalone workspaces: keep every dependency inside the function.
export function createNativeSourceProjectCodec(overrides = {}) {
  const MiB = 1024 * 1024;
  const schema = 'fmd-native-source-project-v1';
  const maximums = {sourceBytes: 32 * MiB, assetBytes: 32 * MiB,
    totalAssetBytes: 128 * MiB, imageCount: 4096, destinationBytes: 8192,
    totalDestinationBytes: 65536, outputBytes: 256 * MiB};
  const slots = ['body-regular', 'body-bold', 'body-italic', 'body-bold-italic', 'mono-regular'];
  const settingKeys = ['font', 'darkMode', 'fontScale', 'title', 'author', 'lang',
    'metadataEpochSeconds', 'pageNumbers', 'codeLineNumbers', 'toc', 'tocDepth', 'pageGeometry'];
  const fail = (code, message) => { throw Object.assign(new Error(message), {code}); };
  const invalid = message => fail('INVALID_SOURCE_PROJECT', message);
  const exceeded = label => fail('SOURCE_PROJECT_LIMIT', label + ' exceeds the source-project limit');

  function record(value, allowed) {
    if (!value || typeof value !== 'object' || Array.isArray(value)) invalid('Expected a plain project data object');
    const prototype = Object.getPrototypeOf(value);
    if (prototype !== null && Object.getPrototypeOf(prototype) !== null) invalid('Expected a plain project data object');
    const keys = Reflect.ownKeys(value);
    if (keys.length > allowed.length) invalid('Unsupported project fields');
    const copy = Object.create(null);
    for (const key of keys) {
      const field = Object.getOwnPropertyDescriptor(value, key);
      if (!allowed.includes(key) || !field || !Object.hasOwn(field, 'value')) invalid('Unsupported project field or accessor');
      copy[key] = field.value;
    }
    return copy;
  }
  const settings = record(overrides, Object.keys(maximums));
  const limits = Object.freeze(Object.fromEntries(Object.entries(maximums).map(([key, max]) => {
    const value = Object.hasOwn(settings, key) ? settings[key] : max;
    if (!Number.isSafeInteger(value) || value < 1 || value > max) invalid('Project limits may only reduce positive default limits');
    return [key, value];
  })));
  function textSize(value, maximum, label) {
    if (typeof value !== 'string') invalid(label + ' must be a string');
    if (value.length > maximum) exceeded(label);
    let bytes = 0;
    for (const character of value) {
      const cp = character.codePointAt(0);
      if (cp >= 0xd800 && cp <= 0xdfff) invalid(label + ' contains an unpaired surrogate');
      bytes += cp < 128 ? 1 : cp < 2048 ? 2 : cp < 65536 ? 3 : 4;
      if (bytes > maximum) exceeded(label);
    }
    return bytes;
  }
  function array(value, maximum, label) {
    if (!Array.isArray(value)) invalid(label + ' must be an array');
    if (value.length > maximum) exceeded(label);
    if (Reflect.ownKeys(value).length !== value.length + 1) invalid(label + ' must be a dense data array');
    const copy = [];
    for (let i = 0; i < value.length; i++) {
      const field = Object.getOwnPropertyDescriptor(value, String(i));
      if (!field || !Object.hasOwn(field, 'value')) invalid(label + ' must be a dense data array');
      copy.push(field.value);
    }
    return copy;
  }
  function captureSettings(input) {
    const value = {font: 'sans', darkMode: 'auto', fontScale: 1,
      pageNumbers: false, codeLineNumbers: false, toc: false, ...record(input, settingKeys)};
    if (!['sans', 'serif'].includes(value.font) || !['auto', 'disabled'].includes(value.darkMode)) invalid('Invalid font family or color mode');
    if (!Number.isFinite(value.fontScale) || value.fontScale < 0.5 || value.fontScale > 3) invalid('fontScale must be 0.5..3');
    for (const key of ['pageNumbers', 'codeLineNumbers', 'toc'])
      if (typeof value[key] !== 'boolean') invalid(key + ' must be boolean');
    for (const [key, maximum] of [['title', 65536], ['author', 65536], ['lang', 1024]])
      if (value[key] !== undefined) textSize(value[key], maximum, key);
    if (value.metadataEpochSeconds !== undefined && (!Number.isSafeInteger(value.metadataEpochSeconds) || value.metadataEpochSeconds < 0)) invalid('Invalid explicit metadata epoch');
    if (value.tocDepth !== undefined && (!Number.isInteger(value.tocDepth) || value.tocDepth < 1 || value.tocDepth > 6)) invalid('tocDepth must be 1..6');
    if (value.pageGeometry !== undefined) {
      if (!Array.isArray(value.pageGeometry) || value.pageGeometry.length !== 6) invalid('Page geometry requires six finite numbers');
      const g = array(value.pageGeometry, 6, 'Page geometry');
      if (g.length !== 6 || !g.every(Number.isFinite)) invalid('Page geometry requires six finite numbers');
      const extent = (size, first, second) => Math.fround(Math.fround(Math.fround(size) - Math.fround(first)) - Math.fround(second));
      if (g[0] < 144 || g[1] < 144 || g.some(n => n < 0 || n > 14400)
          || g[0] - g[5] - g[3] < 72 || g[1] - g[2] - g[4] < 72
          || extent(g[0], g[5], g[3]) < 72 || extent(g[1], g[2], g[4]) < 72) invalid('Invalid PDF paper or margins');
      value.pageGeometry = Object.freeze(g.map(n => n === 0 ? 0 : n));
    }
    return Object.freeze(Object.fromEntries(settingKeys.filter(key => value[key] !== undefined).map(key => [key, value[key]])));
  }
  function sextet(code) {
    return code >= 65 && code <= 90 ? code - 65 : code >= 97 && code <= 122 ? code - 71
      : code >= 48 && code <= 57 ? code + 4 : code === 43 ? 62 : code === 47 ? 63 : -1;
  }
  function encodedLength(value) {
    if (typeof value !== 'string' || !value.length || value.length % 4) invalid('Resource bytes must be padded base64');
    if (value.length > Math.ceil(limits.assetBytes / 3) * 4) exceeded('Resource bytes');
    const padding = value.endsWith('==') ? 2 : value.endsWith('=') ? 1 : 0;
    const size = value.length / 4 * 3 - padding, end = value.length - padding;
    if (!size || size > limits.assetBytes) exceeded('Resource bytes');
    for (let i = 0; i < end; i++) if (sextet(value.charCodeAt(i)) < 0) invalid('Resource bytes must be canonical base64');
    if ((padding === 2 && (sextet(value.charCodeAt(end - 1)) & 15))
        || (padding === 1 && (sextet(value.charCodeAt(end - 1)) & 3))) invalid('Resource bytes must be canonical base64');
    return size;
  }
  function snapshot(input, requireSchema = false) {
    const value = record(input, ['schema', 'source', 'options', 'images', 'fonts']);
    if ((requireSchema || value.schema !== undefined) && value.schema !== schema)
      fail('UNSUPPORTED_SOURCE_PROJECT', 'Unsupported native source-project schema');
    const sourceLength = textSize(value.source, limits.sourceBytes, 'Markdown source');
    const options = captureSettings(value.options);
    // Counts and aggregate byte sizes are admitted before any binary decoding.
    const imageInput = array(value.images, limits.imageCount, 'Images');
    const fontInput = array(value.fonts, slots.length, 'Fonts');
    const images = [], fonts = [], destinations = new Set(), fontSlots = new Set();
    let total = 0, names = 0;
    for (const item of imageInput) {
      const image = record(item, ['destination', 'bytes']);
      const name = image.destination;
      names += textSize(name, limits.destinationBytes, 'Image destination');
      if (!name || name !== name.trim() || /[\u0000-\u001f\u007f-\u009f]/u.test(name) || destinations.has(name)) invalid('Image destinations must be unique, nonempty, trimmed and control-free');
      if (names > limits.totalDestinationBytes) exceeded('Image destinations');
      destinations.add(name);
      total += encodedLength(image.bytes);
      if (total > limits.totalAssetBytes) exceeded('Combined image and font bytes');
      images.push(Object.freeze({destination: name, bytes: image.bytes}));
    }
    for (const item of fontInput) {
      const font = record(item, ['slot', 'bytes', 'weight']);
      if (!slots.includes(font.slot) || fontSlots.has(font.slot)) invalid('Font slots must be recognized and unique');
      if (font.weight !== undefined && (!Number.isInteger(font.weight) || font.weight < 1 || font.weight > 1000)) invalid('Font weight must be 1..1000');
      fontSlots.add(font.slot);
      total += encodedLength(font.bytes);
      if (total > limits.totalAssetBytes) exceeded('Combined image and font bytes');
      fonts.push(Object.freeze({slot: font.slot, bytes: font.bytes,
        ...(font.weight === undefined ? {} : {weight: font.weight})}));
    }
    fonts.sort((a, b) => slots.indexOf(a.slot) - slots.indexOf(b.slot));
    return {sourceLength, data: Object.freeze({schema, source: value.source, options,
      images: Object.freeze(images), fonts: Object.freeze(fonts)})};
  }
  function quotedSize(value) {
    let size = 2;
    for (const character of value) {
      const cp = character.codePointAt(0);
      size += cp === 34 || cp === 92 || cp === 8 || cp === 9 || cp === 10 || cp === 12 || cp === 13 ? 2
        : cp < 32 || cp === 60 || cp === 0x2028 || cp === 0x2029 ? 6
        : cp < 128 ? 1 : cp < 2048 ? 2 : cp < 65536 ? 3 : 4;
    }
    return size;
  }
  function admitJson(value) {
    let bytes = 0;
    const add = count => { bytes += count; if (bytes > limits.outputBytes) exceeded('Project JSON'); };
    function visit(item) {
      if (typeof item === 'string') { add(quotedSize(item)); return; }
      if (typeof item !== 'object') { add(JSON.stringify(item).length); return; }
      if (Array.isArray(item)) {
        add(2 + Math.max(0, item.length - 1));
        for (const entry of item) visit(entry);
      } else {
        const keys = Object.keys(item);
        add(2 + Math.max(0, keys.length - 1));
        for (const key of keys) { add(quotedSize(key) + 1); visit(item[key]); }
      }
    }
    visit(value);
    return bytes;
  }
  function fileView(input) {
    let buffer = input, offset = 0, length;
    try {
      if (ArrayBuffer.isView(input)) {
        const typed = Object.getPrototypeOf(Uint8Array.prototype);
        let getters = typed;
        try { Object.getOwnPropertyDescriptor(typed, 'buffer').get.call(input); }
        catch { getters = DataView.prototype; }
        const read = key => Object.getOwnPropertyDescriptor(getters, key).get.call(input);
        buffer = read('buffer'); offset = read('byteOffset'); length = read('byteLength');
      }
      const size = Object.getOwnPropertyDescriptor(ArrayBuffer.prototype, 'byteLength').get.call(buffer);
      const view = new Uint8Array(buffer, offset, length ?? size);
      if (view.length > limits.outputBytes) exceeded('Project JSON');
      return view;
    } catch (error) {
      if (error?.code === 'SOURCE_PROJECT_LIMIT') throw error;
      invalid('Project input must be UTF-8 text or a non-shared byte buffer');
    }
  }
  function decode(input) {
    let text;
    if (typeof input === 'string') {
      textSize(input, limits.outputBytes, 'Project JSON'); text = input;
    } else {
      try { text = new TextDecoder('utf-8', {fatal: true, ignoreBOM: true}).decode(fileView(input)); }
      catch (error) {
        if (['INVALID_SOURCE_PROJECT', 'SOURCE_PROJECT_LIMIT'].includes(error?.code)) throw error;
        invalid('Project JSON must contain valid UTF-8');
      }
    }
    // An optional file BOM is not part of the source field, whose own BOM and
    // original newlines remain exact. No reviver, object hooks or eval are used.
    if (text.charCodeAt(0) === 0xfeff) text = text.slice(1);
    let value;
    try { value = JSON.parse(text); } catch { invalid('Invalid source-project JSON'); }
    return snapshot(value, true).data;
  }
  function filename(base = 'document') {
    if (typeof base !== 'string') invalid('Project filename must be a string');
    let name = Array.from(base).slice(0, 80).join('').replace(/[<>:"/\\|?*\u0000-\u001f\u007f]/g, '_')
      .trim().replace(/[. ]+$/, '');
    if (!name || /^(con|prn|aux|nul|com[1-9]|lpt[1-9])(?:\.|$)/i.test(name)) name = 'document';
    return name + '.fmd.json';
  }
  function encode(input) {
    const {sourceLength, data} = snapshot(input);
    admitJson(data); // Admission before allocating the aggregate escaped string.
    const json = JSON.stringify(data).replace(/</g, '\\u003c').replace(/\u2028/g, '\\u2028').replace(/\u2029/g, '\\u2029');
    const bytes = new TextEncoder().encode(json);
    return Object.freeze({schema, format: 'native-source-project', extension: 'json',
      mimeType: 'application/json', sourceLength, bytes, filename,
      text: () => new TextDecoder('utf-8', {fatal: true}).decode(bytes),
      blob: () => new Blob([bytes], {type: 'application/json'})});
  }
  function decodeResource(text) {
    const bytes = new Uint8Array(encodedLength(text));
    let offset = 0;
    // Limit temporary binary strings; resource validation has already finished.
    for (let i = 0; i < text.length; i += 32768) {
      const binary = atob(text.slice(i, i + 32768));
      for (let j = 0; j < binary.length; j++) bytes[offset++] = binary.charCodeAt(j);
    }
    return bytes;
  }
  function renderOptions(input) {
    const {data} = snapshot(input, true), options = {...data.options, allowRawHtml: false};
    const geometry = options.pageGeometry;
    delete options.pageGeometry;
    if (geometry !== undefined) {
      const [widthPt, heightPt, topPt, rightPt, bottomPt, leftPt] = geometry;
      options.page = {size: {widthPt, heightPt}, margins: {topPt, rightPt, bottomPt, leftPt}};
    }
    options.pdfImages = data.images.map(image => ({destination: image.destination, bytes: decodeResource(image.bytes)}));
    options.fontAssets = data.fonts.map(font => ({slot: font.slot, bytes: decodeResource(font.bytes),
      ...(font.weight === undefined ? {} : {weight: font.weight})}));
    return options;
  }
  return Object.freeze({limits, encode, decode, renderOptions});
}

const codec = createNativeSourceProjectCodec();
export const NATIVE_SOURCE_PROJECT_LIMITS = codec.limits;

/** Decode data only. The caller explicitly chooses a renderer and where to save.
 * Options, resource arrays and byte views are fresh, editable host-owned copies. */
export function readNativeSourceProject(input) {
  const data = codec.decode(input);
  return Object.freeze({source: data.source, options: codec.renderOptions(data)});
}
