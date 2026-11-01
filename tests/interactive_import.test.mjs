// Exercise the shipped import pipeline, not a separate test implementation.
import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
import {test} from 'node:test';
import vm from 'node:vm';
import {deflateSync} from 'node:zlib';
const context = vm.createContext({btoa, Uint8Array, Map, TextDecoder});
for (const name of ['interactive_renderer.js', 'interactive_import.js']) {
  vm.runInContext(readFileSync(new URL('../src/' + name, import.meta.url), 'utf8'), context);
}
const {fmdImportImageInfo: info, fmdPrepareImageImport: prepare, fmdImportAlt: alt, fmdImportBase64: base64} = context;
const limits = vm.runInContext('FMD_IMAGE_IMPORT_LIMITS', context);
function chunk(tag, data) {
  const result = Buffer.alloc(data.length + 12);
  result.writeUInt32BE(data.length); result.write(tag, 4); data.copy(result, 8);
  // This harness tests admission, not a decoder; Chromium tests use real CRCs.
  return result;
}
function png(width = 2, height = 1, animated = false) {
  const ihdr = Buffer.alloc(13); ihdr.writeUInt32BE(width); ihdr.writeUInt32BE(height, 4); ihdr[8] = 8; ihdr[9] = 6;
  return Buffer.concat([Buffer.from([137,80,78,71,13,10,26,10]), chunk('IHDR', ihdr),
    ...(animated ? [chunk('acTL', Buffer.alloc(8))] : []), chunk('IDAT', deflateSync(Buffer.from([0,255,0,0,255,0,255,0,255]))), chunk('IEND', Buffer.alloc(0))]);
}
function jpeg(sof = 192, width = 2, height = 1) {
  const frame = Buffer.from([255,216,255,224,0,4,1,2,255,sof,0,11,8,0,0,0,0,1,1,0x11,0,255,217]);
  frame.writeUInt16BE(height, 13); frame.writeUInt16BE(width, 15); return frame;
}
const gifFixture = name => Buffer.from(readFileSync(new URL('./fixtures/gif/' + name + '.base64', import.meta.url), 'utf8'), 'base64');
function tinyGif({version = 'GIF87a', width = 2, height = 2, left = 0, top = 0,
    global = true, local = false, interlaced = false, transparent = null} = {}) {
  const screen = Buffer.alloc(7); screen.writeUInt16LE(width); screen.writeUInt16LE(height, 2); screen[4] = global ? 128 : 0;
  const descriptor = Buffer.from([0x2c, 0, 0, 0, 0, 1, 0, 1, 0, (local ? 128 : 0) | (interlaced ? 64 : 0)]);
  descriptor.writeUInt16LE(left, 1); descriptor.writeUInt16LE(top, 3);
  const colors = Buffer.from([255, 0, 0, 0, 255, 0]);
  return Buffer.concat([Buffer.from(version), screen, ...(global ? [colors] : []),
    ...(transparent === null ? [] : [Buffer.from([0x21, 0xf9, 4, 1, 0, 0, transparent, 0])]),
    descriptor, ...(local ? [colors] : []), Buffer.from([2, 2, 0x44, 0x01, 0, 0x3b])]);
}
function file(name, bytes, extra = {}) {
  return {name, size: bytes.length, async arrayBuffer() { return Uint8Array.from(bytes).buffer; }, ...extra};
}
const verify = async () => {}, current = () => true;

test('PNG dimensions and byte-determined MIME are admitted before decoding', () => {
  assert.equal(JSON.stringify(info(png(100, 200))), JSON.stringify({mime:'image/png', width:100, height:200}));
});
test('baseline and progressive JPEG frames resolve after metadata segments', () => {
  for (const sof of [192,193,194]) assert.equal(JSON.stringify(info(jpeg(sof, 640, 480))), JSON.stringify({mime:'image/jpeg', width:640, height:480}));
});
test('GIF87a/89a global and local palettes, offsets, interlace and transparency preserve canvas dimensions', () => {
  for (const version of ['GIF87a', 'GIF89a']) {
    for (const options of [{}, {global:false,local:true}, {local:true,interlaced:true},
      {left:1,top:1,transparent:0}, {transparent:255}]) {
      assert.equal(JSON.stringify(info(tinyGif({version,...options}))), JSON.stringify({mime:'image/gif',width:2,height:2}));
    }
  }
});
test('independent animated and interlaced GIF fixtures retain exact bytes through portable import', async () => {
  for (const [name, size] of [['animated-colors', 2], ['interlaced-palette', 96]]) {
    const bytes = gifFixture(name), verified = [];
    assert.equal(JSON.stringify(info(bytes)), JSON.stringify({mime:'image/gif',width:size,height:size}));
    const text = await prepare([file('renamed.png',bytes,{type:'image/png'})],async(uri,meta)=>verified.push(meta.mime),current);
    assert.deepEqual(verified,['image/gif']);
    assert.equal(text,'![renamed.png](data:image/gif;base64,'+bytes.toString('base64')+')');
    assert.match(context.parseMarkdownClient(text), /<img[^>]+src="data:image\/gif;base64,/);
  }
});
test('GIF extension and image data sub-blocks may span multiple bounded chunks', () => {
  const bytes = tinyGif();
  const comment = Buffer.from([0x21,0xfe,2,65,66,1,67,0]);
  const splitData = Buffer.from([2,1,0x44,1,0x01,0,0x3b]);
  const split = Buffer.concat([bytes.subarray(0,19),comment,bytes.subarray(19,29),splitData]);
  assert.equal(info(split).mime,'image/gif');
});
test('GIF canvas limits and frame bounds are checked before decoding', async () => {
  let decodes = 0;
  for (const bytes of [tinyGif({width:0}),tinyGif({height:0}),tinyGif({width:16385}),
    tinyGif({width:5000,height:5000}),tinyGif({left:2}),tinyGif({top:2})]) {
    await assert.rejects(prepare([file('unsafe.gif',bytes)],async()=>decodes++,current), /dimensions/);
  }
  assert.equal(decodes,0);
});
test('GIF truncation, missing palettes, malformed controls and invalid image framing are refused', () => {
  for (const bytes of [tinyGif(),tinyGif({global:false,local:true}),tinyGif({transparent:0}),gifFixture('animated-colors')]) {
    for (let end = 1; end < bytes.length; end++) assert.throws(()=>info(bytes.subarray(0,end)), 'truncated at '+end);
    assert.throws(()=>info(Buffer.concat([bytes,Buffer.from([0])])), /GIF/);
  }
  assert.throws(()=>info(tinyGif({global:false})), /GIF/);
  const badBackground=tinyGif();badBackground[11]=2;assert.throws(()=>info(badBackground), /GIF/);
  const badLocal=tinyGif({global:false,local:true});badLocal[22]=0x87;assert.throws(()=>info(badLocal), /GIF/);
  for (const size of [0,1,9,255]) {
    const bad=tinyGif();bad[29]=size;assert.throws(()=>info(bad), /GIF/);
  }
  const noData=tinyGif();noData[30]=0;assert.throws(()=>info(noData), /GIF/);
  const noFrame=Buffer.concat([tinyGif().subarray(0,19),Buffer.from([0x3b])]);assert.throws(()=>info(noFrame), /GIF/);
  const zeroFrame=tinyGif();zeroFrame[24]=0;assert.throws(()=>info(zeroFrame), /dimensions/);
  const badControl=tinyGif({transparent:0});badControl[21]=3;assert.throws(()=>info(badControl), /GIF/);
  const badTerminator=tinyGif({transparent:0});badTerminator[26]=1;assert.throws(()=>info(badTerminator), /GIF/);
  const text=Buffer.concat([tinyGif().subarray(0,19),Buffer.from([0x21,0x01,12]),Buffer.alloc(13),tinyGif().subarray(19)]);
  assert.throws(()=>info(text), /plain-text/);
});
test('PNG zero, excessive-axis and excessive-pixel dimensions fail early', () => {
  for (const [w,h] of [[0,1],[1,0],[16385,1],[5000,5000],[0xffffffff,0xffffffff]]) assert.throws(() => info(png(w,h)), /dimensions/);
});
test('JPEG dimensions are bounded too', () => {
  for (const [w,h] of [[0,1],[1,0],[16385,1],[5000,5000]]) assert.throws(() => info(jpeg(192,w,h)), /dimensions/);
});
test('static PNG requirement refuses animation before decoding', () => {
  assert.throws(() => info(png(2,1,true)), /Animated PNG/);
});
test('truncated, duplicated, trailing and malformed PNG chunks cannot masquerade as an image', () => {
  const bytes = png();
  for (const length of [1,8,24,32,bytes.length-1]) assert.throws(() => info(bytes.subarray(0,length)));
  assert.throws(() => info(Buffer.concat([bytes,Buffer.from([0])])));
  const size = Buffer.from(bytes); size.writeUInt32BE(0xffffffff,33); assert.throws(() => info(size));
  assert.throws(() => info(Buffer.concat([bytes.subarray(0,33),bytes.subarray(8)])));
  assert.throws(() => info(Buffer.concat([bytes.subarray(0,33),chunk('IEND',Buffer.alloc(0))])));
});
test('malformed JPEG marker lengths, frame counts and absent frames are refused', () => {
  for (const bytes of [Buffer.from([255,216,255,217]),Buffer.from([255,216,255,224,0,0]),
    Buffer.from([255,216,255,224,255,255]),Buffer.from([255,216,255,218,0,2]),
    Buffer.from([255,216,255,255,255,255])]) assert.throws(() => info(bytes));
  const bytes = jpeg(); bytes[17] = 2; assert.throws(() => info(bytes));
});
test('file type and extension cannot authorize HTML, active SVG or unsupported formats', async () => {
  let decoded = 0;
  for (const bytes of [Buffer.from('<svg onload="alert(1)"/>'),Buffer.from('<html>bad</html>'),Buffer.from('RIFFxxxxWEBP')]) {
    await assert.rejects(prepare([file('claimed.png',bytes,{type:'image/png'})],async () => decoded++,current), /SVG/);
  }
  assert.equal(decoded, 0);
});
test('one and many chunk base64 encodings round-trip exactly', () => {
  for (const length of [0,1,2,3,16383,16384,16385,65537]) {
    const bytes = Uint8Array.from({length}, (_,i) => (i*37)%256);
    assert.equal(base64(bytes),Buffer.from(bytes).toString('base64'));
  }
});
test('filenames become literal bounded alt text rather than HTML, links or images', async () => {
  const name = '[!<script>&amp;_*`\\\n.png';
  const text = await prepare([file(name,png())],verify,current);
  const html = context.parseMarkdownClient(text);
  assert.equal((html.match(/<img /g)||[]).length,1);
  assert.doesNotMatch(html, /<script>|<a |<em>|<strong>/);
  assert.match(html,/&amp;amp;/);
  assert.equal(alt(''), 'image'); assert.equal(Array.from(alt('é'.repeat(300))).length,200);
});
test('preflight refuses count, individual and aggregate byte limits without reading', async () => {
  let reads = 0;
  const f = size => ({size,async arrayBuffer(){reads++;return new ArrayBuffer(0);}});
  for (const files of [[],Array(9).fill(f(1)),[f(0)],[f(-1)],[f(1.5)],[f(Infinity)],
    [f(limits.fileBytes+1)],[f(limits.fileBytes),f(limits.fileBytes),f(1)]]) await assert.rejects(prepare(files,verify,current));
  assert.equal(reads,0);
});
test('a batch preserves file order, actual MIME and exact encoded bytes', async () => {
  const one = png(), two = jpeg(); const decoded=[];
  const result = await prepare([file('renamed.jpg',one),file('second.png',two)],async(uri,meta)=>decoded.push(meta.mime),current);
  assert.deepEqual(decoded,['image/png','image/jpeg']);
  assert.equal(result,'![renamed.jpg](data:image/png;base64,'+one.toString('base64')+')\n\n![second.png](data:image/jpeg;base64,'+two.toString('base64')+')');
});
test('size changes and read failures stop before decoding', async () => {
  let decodes=0;
  await assert.rejects(prepare([file('x',png(),{size:1})],async()=>decodes++,current),/size changed/);
  await assert.rejects(prepare([file('x',png(),{async arrayBuffer(){throw Error('unreadable');}})],async()=>decodes++,current),/unreadable/);
  assert.equal(decodes,0);
});
test('browser decoding failure prevents any prepared batch from being returned', async () => {
  let calls=0;
  await assert.rejects(prepare([file('good',png()),file('bad',jpeg())],async()=>{ if(++calls===2) throw Error('decode failed'); },current),/decode failed/);
  assert.equal(calls,2);
});
test('stale revisions are refused before reads and after each asynchronous boundary', async () => {
  let stale=false, reads=0, decodes=0;
  const bytes=png();
  await assert.rejects(prepare([file('x',bytes)],verify,()=>false),/document changed/);
  await assert.rejects(prepare([file('x',bytes,{async arrayBuffer(){reads++; stale=true; return Uint8Array.from(bytes).buffer;}})],async()=>decodes++,()=>!stale),/document changed/);
  assert.equal(reads,1); assert.equal(decodes,0);
  stale=false;
  await assert.rejects(prepare([file('x',bytes),file('later',bytes)],async()=>{decodes++;stale=true;},()=>!stale),/document changed/);
  assert.equal(decodes,1);
});
test('metadata dimensions reject oversized content before browser decoding', async () => {
  let decoded=false;
  await assert.rejects(prepare([file('huge.png',png(5000,5000))],async()=>{decoded=true;},current),/dimensions/);
  assert.equal(decoded,false);
});
