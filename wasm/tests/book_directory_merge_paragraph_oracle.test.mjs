import test from "node:test";
import assert from "node:assert/strict";
import { createBookSourceMerger as make } from "../demo/book_source_merge.mjs";

// Independent test oracle: enumerate every longest matching coordinate sequence,
// then compare every pair of edit interpretations. Production uses bounded
// forward/reverse diffs, not this intentionally exponential tiny-input oracle.
function align(a,b) {
 const memo = new Map();
 function solve(i,j) {
  const key=i+':'+j; if(memo.has(key)) return memo.get(key);
  if(i===a.length || j===b.length) return [[]];
  const found=[];
  if(a[i]===b[j]) for(const rest of solve(i+1,j+1)) found.push([[i,j],...rest]);
  found.push(...solve(i+1,j),...solve(i,j+1));
  const len=Math.max(...found.map(x=>x.length));
  const out=[...new Map(found.filter(x=>x.length===len).map(x=>[JSON.stringify(x),x])).values()];
  memo.set(key,out); return out;
 }
 return solve(0,0).map(matches=>{
  let i=0,j=0; const out=[];
  for(const [ai,bj] of [...matches,[a.length,b.length]]) {
   if(ai!==i || bj!==j) out.push({start:i,end:ai,lines:b.slice(j,bj)});
   i=ai+1; j=bj+1;
  }
  return out;
 });
}
const overlaps=(a,b)=> a.start===a.end? a.start>=b.start&&a.start<=b.end : b.start===b.end? b.start>=a.start&&b.start<=a.end : a.start<b.end&&b.start<a.end;
function combined(base,a,b){
 const edits=[...a];
 for(const y of b){
  const os=a.filter(x=>overlaps(x,y));
  if(os.some(x=>JSON.stringify(x)!==JSON.stringify(y))) return null;
  if(!os.length) edits.push(y);
 }
 edits.sort((x,y)=>x.start-y.start); let at=0;const out=[];
 for(const e of edits){out.push(...base.slice(at,e.start),...e.lines);at=e.end;}out.push(...base.slice(at));return out.join('');
}
test("all admitted merges in 24,360 repeated-word triples agree with every shortest alignment", t => {
  const words = [];
  for (let n = 1; n <= 4; n++) for (let i = 0; i < 2 ** n; i++)
    words.push(Array.from({ length: n }, (_, j) => (i >> j) & 1 ? "b" : "a").join(" "));
  const tokens = text => text.match(/ +|[^ ]+/g) ?? [];
  let admitted = 0, checks = 0, cases = 0;
  for (const base of words) for (const local of words) for (const disk of words) {
    if (local === base || disk === base || local === disk) continue;
    cases++;
    const actual = make()(base, local, disk);
    if (actual.action !== "merged") continue;
    admitted++;
    const b = tokens(base), left = align(b, tokens(local)), right = align(b, tokens(disk));
    for (const l of left) for (const r of right) {
      checks++;
      assert.equal(combined(b, l, r), actual.source, JSON.stringify({ base, local, disk, l, r }));
    }
  }
  assert.equal(cases, 24360);
  assert(admitted > 0); assert(checks >= admitted);
  t.diagnostic(`${cases} triples; ${admitted} admitted merges; ${checks} shortest-alignment pairs checked`);
});
