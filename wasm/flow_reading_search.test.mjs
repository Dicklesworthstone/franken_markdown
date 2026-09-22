import assert from "node:assert/strict";
import test from "node:test";
import { FlowReadingError, readFlowDocument } from "./flow_reading.mjs";
import { CASE_FOLD_VERSION, foldScalar } from "./flow_reading.mjs";
const code = value => error => error instanceof FlowReadingError && error.code === value;
const node = (text, children = [], role = "paragraph") => ({ role, text, children,
  bounds: { x: 0, y: 0, width: 100, height: 20 }, enclosingSourceSpan: { startByte: 0, endByte: 10 } });
async function fixture(texts) {
  const roots = texts.map(text => typeof text === "string" ? node(text) : text);
  const session = { disposed: false, token: { revision: "1", layoutRevision: "1" }, calls: 0,
    readingOrder({offset, limit}) { this.calls++; const end = Math.min(offset + limit, roots.length);
      return {schemaVersion: 1, ...this.token, offset, total: roots.length,
        nextOffset: end < roots.length ? end : null, nodes: structuredClone(roots.slice(offset, end))};
    } };
  return { session, document: await readFlowDocument(session) };
}
const positions = result => result.matches.map(({nodeIndex, startUtf16, endUtf16}) => [nodeIndex, startUtf16, endUtf16]);
const strings = (document, result) => result.matches.map(m => document.matchText(m));

test("default and ASCII searches preserve case sensitivity and original scalar ranges", async () => {
  const {document} = await fixture(["aAA éÉ 🙂a🙂a"]);
  assert.deepEqual(positions(document.find("AA")), [[0,1,3]]);
  assert.deepEqual(positions(document.find("aa", {asciiCaseInsensitive:true})), [[0,0,2]]);
  assert.deepEqual(strings(document, document.find("é",{asciiCaseInsensitive:true})), ["é"]);
  assert.deepEqual(positions(document.find("🙂a")), [[0,7,10],[0,10,13]]);
  assert.deepEqual(document.find(""), { matches: [], truncated:false });
});

test("full folding matches sharp s, ligatures, final sigma, accents and supplementary letters", async () => {
  const {document} = await fixture(["Straße STRASSE ﬃ FFI Σςσ Éé 𐐀𐐨 ꭰᎠ"]);
  for (const [query, expected] of [["strasse",["Straße","STRASSE"]], ["ffi",["ﬃ","FFI"]],
    ["Σ",["Σ","ς","σ"]], ["é",["É","é"]], ["𐐨",["𐐀","𐐨"]], ["Ꭰ",["ꭰ","Ꭰ"]]]) {
    const result = document.find(query,{caseInsensitive:true});
    assert.deepEqual(strings(document, result), expected, query);
    assert.deepEqual(positions(await document.findAsync(query,{caseInsensitive:true})), positions(result));
  }
  assert.equal(CASE_FOLD_VERSION,"15.1.0");
});

test("partial fold expansions are not selectable matches and do not hide following matches", async () => {
  const {document} = await fixture(["ßs ﬃfffi İi i\u0307"]);
  assert.deepEqual(strings(document,document.find("s",{caseInsensitive:true})),["s"]);
  assert.deepEqual(strings(document,document.find("ff",{caseInsensitive:true})),["ff"]);
  assert.deepEqual(strings(document,document.find("ffi",{caseInsensitive:true})),["ﬃ","ffi"]);
  assert.deepEqual(strings(document,document.find("i",{caseInsensitive:true})),["i","i","i"]);
  assert.deepEqual(strings(document,document.find("İ",{caseInsensitive:true})),["İ","i\u0307"]);
});

test("case folding is neither locale tailoring nor canonical or accent normalization", async () => {
  const {document} = await fixture(["I i ı İ é e\u0301 e"]);
  assert.deepEqual(strings(document,document.find("I",{caseInsensitive:true})),["I","i"]);
  assert.deepEqual(strings(document,document.find("é",{caseInsensitive:true})),["é"]);
  assert.deepEqual(strings(document,document.find("e\u0301",{caseInsensitive:true})),["e\u0301"]);
});

test("whole-word matching includes Unicode letters, marks, numbers, connectors and joiners", async () => {
  const {document} = await fixture(["cat scatter cat_cat cat2 cat\u0301 cat\u200d cat-cat 猫cat cat猫 🙂cat🙂 CAT"]);
  assert.deepEqual(strings(document,document.find("cat", {wholeWord:true,caseInsensitive:true})),["cat","cat","cat","cat","CAT"]);
});

test("whole-word rejection preserves a later valid overlapping-prefix candidate", async () => {
  const {document} = await fixture(["xaba aba ababa aba"]);
  assert.deepEqual(positions(document.find("aba",{wholeWord:true})),[[0,5,8],[0,15,18]]);
});

test("search skips container transcripts and never joins semantic leaves", async () => {
  const {document} = await fixture([node("Straße Straße",[node("Straße",[],"table-cell"),node("Straße",[],"table-cell")],"table-row"),"str","asse"]);
  const result = await document.findAsync("STRASSE",{caseInsensitive:true});
  assert.deepEqual(result.matches.map(m=>m.nodeIndex),[1,2]);
  assert.equal(document.find("strasse").matches.length,0);
});

test("non-overlap and truncation count genuine extra matches, not rejected expansions", async () => {
  const {document} = await fixture(["ßSSßß"]);
  const opts={caseInsensitive:true,maxMatches:3};
  assert.deepEqual(strings(document,document.find("ss",opts)),["ß","SS","ß"]);
  assert.equal(document.find("ss",opts).truncated,true);
  assert.equal(document.find("ss",{...opts,maxMatches:4}).truncated,false);
  assert.equal(document.find("s",{...opts,maxMatches:2}).truncated,false);
});

test("case folds and matches cross cooperative boundaries with correct original UTF-16", async () => {
  for (const prefix of [4092,4093,4094,4095,4096,8191]) {
    const source = "x".repeat(prefix)+"🙂Straßeﬃ🙂";
    const {document} = await fixture([source]);
    const result=await document.findAsync("🙂STRASSEffi🙂",{caseInsensitive:true});
    assert.deepEqual(positions(result),[[0,prefix,source.length]]);
    assert.equal(document.matchText(result.matches[0]),"🙂Straßeﬃ🙂");
  }
});

test("returned matches remain immutable and bound to their actual document", async () => {
  const first=await fixture(["Straße"]), second=await fixture(["Straße"]);
  const result=await first.document.findAsync("STRASSE",{caseInsensitive:true});
  assert(Object.isFrozen(result)&&Object.isFrozen(result.matches)&&Object.isFrozen(result.matches[0]));
  assert.throws(()=>second.document.matchText(result.matches[0]),code("INVALID_ARGUMENT"));
  assert.throws(()=>first.document.matchText({...result.matches[0]}),code("INVALID_ARGUMENT"));
  first.session.token.layoutRevision="2";
  assert.throws(()=>first.document.matchText(result.matches[0]),code("STALE_LAYOUT"));
});

test("abort interrupts a huge single leaf without issuing more native requests", async () => {
  const {document,session}=await fixture(["x".repeat(1000000)]), control=new AbortController();
  const pending=document.findAsync("needle",{signal:control.signal}), refused=assert.rejects(pending,code("ABORTED"));
  const timer=setTimeout(()=>control.abort(),0);
  try{await refused;}finally{clearTimeout(timer);}
  assert.equal(session.calls,1);assert.equal(session.disposed,false);
});

test("source, layout, and disposal invalidate suspended searches without publishing results", async () => {
  for(const change of [s=>s.token.revision="2",s=>s.token.layoutRevision="2",s=>s.disposed=true]){
    const {document,session}=await fixture(["x".repeat(1000000)]);
    const pending=document.findAsync("needle"),refused=assert.rejects(pending,error=>
      ["STALE_REVISION","STALE_LAYOUT","SESSION_DISPOSED"].includes(error.code));
    const timer=setTimeout(()=>change(session),0);
    try{await refused;}finally{clearTimeout(timer);}
  }
});

test("aborted empty searches and malformed options fail before publishing", async () => {
  const {document}=await fixture(["text"]);
  await assert.rejects(document.findAsync("",{signal:AbortSignal.abort()}),code("ABORTED"));
  for(const opts of [{maxMatches:0},{maxMatches:1001},{caseInsensitive:1},{wholeWord:"yes"},
    {asciiCaseInsensitive:true,caseInsensitive:true},{unexpected:true}]){
    assert.throws(()=>document.find("x",opts));await assert.rejects(document.findAsync("x",opts));
  }
  for(const query of [null,4,"a".repeat(1025),"\ud800","\udfff"]){
    assert.throws(()=>document.find(query));await assert.rejects(document.findAsync(query));
  }
  assert.throws(()=>document.find("a",{signal:AbortSignal.abort()}),code("INVALID_OPTIONS"));
  await assert.rejects(document.findAsync("a",{signal:{}}),code("INVALID_OPTIONS"));
});

test("small searches avoid timers; cancellation releases scheduled listeners", async () => {
  const {document}=await fixture(["short"]),original=setTimeout;let timers=0;
  globalThis.setTimeout=(...args)=>{timers++;return original(...args);};
  try{await document.findAsync("short");}finally{globalThis.setTimeout=original;}
  assert.equal(timers,0);
  const big=await fixture(["x".repeat(100000)]),c=new AbortController();let adds=0,removes=0;
  const add=c.signal.addEventListener.bind(c.signal),remove=c.signal.removeEventListener.bind(c.signal);
  c.signal.addEventListener=(...args)=>{adds++;return add(...args);};
  c.signal.removeEventListener=(...args)=>{removes++;return remove(...args);};
  const pending=big.document.findAsync("absent",{signal:c.signal});c.abort();
  await assert.rejects(pending,code("ABORTED"));assert.equal(adds,removes);
});

// Independent whole-string oracle retains offsets for comparison only. The
// production implementation must not allocate a document-sized folded string.
function reference(source,query,wholeWord,max=1000){
  let folded="";const boundaries=new Map([[0,0]]);let sourceOffset=0;
  for(const scalar of source){folded+=foldScalar(scalar.codePointAt(0));sourceOffset+=scalar.length;boundaries.set(folded.length,sourceOffset);}
  let needle="";for(const scalar of query)needle+=foldScalar(scalar.codePointAt(0));
  const found=[];if(!needle)return {found,truncated:false};
  const word=/[\p{L}\p{M}\p{N}\p{Pc}\u200c\u200d]/u;
  for(let from=0;from<=folded.length-needle.length;){
    const index=folded.indexOf(needle,from);if(index<0)break;
    const end=index+needle.length,startSource=boundaries.get(index),endSource=boundaries.get(end);
    const before=startSource===undefined?"":Array.from(source.slice(0,startSource)).at(-1)??"";
    const after=endSource===undefined?"":Array.from(source.slice(endSource))[0]??"";
    if(startSource!==undefined&&endSource!==undefined&&(!wholeWord||(!word.test(before)&&!word.test(after)))){
      if(found.length===max)return {found,truncated:true};
      found.push([0,startSource,endSource]);from=end;
    }else from=index+1;
  }return {found,truncated:false};
}

test("seeded folded searches match an independent whole-string boundary oracle", async () => {
  let state=1729;const rand=n=>{state=(Math.imul(state,1664525)+1013904223)>>>0;return state%n;};
  const alphabet=["a","A","s","ß","Σ","ς","ﬃ","I","İ","ı","𐐀","🙂","e","é","\u0307"," ","_","-"];
  for(let round=0;round<300;round++){
    const source=Array.from({length:60},()=>alphabet[rand(alphabet.length)]).join("");
    const query=Array.from({length:1+rand(5)},()=>alphabet[rand(alphabet.length)]).join("");
    const wholeWord=!!rand(2),maxMatches=1+rand(8),{document}=await fixture([source]);
    const expected=reference(source,query,wholeWord,maxMatches);
    const result=document.find(query,{caseInsensitive:true,wholeWord,maxMatches});
    assert.deepEqual(positions(result),expected.found);assert.equal(result.truncated,expected.truncated);
    assert.deepEqual(positions(await document.findAsync(query,{caseInsensitive:true,wholeWord,maxMatches})),expected.found);
  }
});
