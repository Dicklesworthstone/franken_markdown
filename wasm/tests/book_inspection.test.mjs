// Production inspection adapter; explicit engine reports shaped from the
// existing Rust documentStats/accessibilityAudit contracts. No WASM substitute.

import assert from "node:assert/strict";
import test from "node:test";
import { inspectBook, inspectionDownload, readBookInspection } from "../book_inspection.mjs";

const source = "\ufeff# Café 🚀\r\n\r\nA paragraph.\n";
const files = () => [
  { path: "intro.md", source },
  { path: "guide/next.md", source: "# Next" },
];
const encode = (value) => new TextEncoder().encode(JSON.stringify(value));
const parsed = (value) => JSON.parse(new TextDecoder().decode(value.bytes));
const gate = () => {
  let resolve;
  const promise = new Promise((yes) => {
    resolve = yes;
  });
  return { promise, resolve };
};
function stats(source) {
  return {
    schema: "fmd-document-stats-v1",
    bytes: new TextEncoder().encode(source).length,
    lines: 3,
    words: 5,
    characters: 12,
    sentences: 1,
    syllables: 7,
    reading_time_secs: 2,
    speaking_time_secs: 3,
    flesch_reading_ease: 80.5,
    flesch_kincaid_grade: 4.1,
    reading_ease_label: "Easy",
    outline: [{ level: 1, text: "Title", slug: "title" }],
    findings: [],
    structure: Object.fromEntries(
      [
        "headings_total",
        "paragraphs",
        "code_blocks",
        "tables",
        "table_rows",
        "table_cells",
        "lists",
        "list_items",
        "task_items_total",
        "task_items_completed",
        "blockquotes",
        "math_blocks",
        "math_inlines",
        "links_total",
        "links_external",
        "links_internal_anchors",
        "images",
        "footnote_definitions",
        "footnote_references",
      ].map((key) => [key, 1]),
    ),
  };
}
const audit = () => ({
  schema_version: "1",
  target: "pdf",
  findings: [],
  pages: [{ privateText: "not retained" }],
  verdict: "clean",
});
const engine = (changes = {}) => ({
  documentStats: async (source) => stats(source),
  accessibilityAudit: async () => audit(),
  ...changes,
});
async function run(e = engine(), input = files()) {
  return readBookInspection((await inspectBook(e, input)).bytes, input);
}
test("all chapters use the engine APIs in reading order and aggregates are exact", async () => {
  const calls = [],
    e = engine({
      documentStats(s) {
        calls.push(["stats", s]);
        return stats(s);
      },
      accessibilityAudit(s) {
        calls.push(["audit", s]);
        return audit();
      },
    });
  const r = await run(e);
  assert.deepEqual(
    calls.map((v) => v[0]),
    ["stats", "audit", "stats", "audit"],
  );
  assert.deepEqual(
    calls.map((v) => v[1]),
    [source, source, "# Next", "# Next"],
  );
  assert.equal(r.summary.total.words, 10);
  assert.equal(r.summary.total.reading_time_secs, 4);
  assert.equal(r.summary.verdict, "no-findings");
  assert.equal(r.scope, "unexpanded-source-chapters");
  assert.equal(r.summary.totalChapters, 2);
});
test("input source and reading order are snapshotted before any asynchronous work", async () => {
  const f = files(),
    original = files(),
    hold = gate();
  let once = true;
  const pending = inspectBook(
    engine({
      async documentStats(s) {
        if (once) {
          once = false;
          await hold.promise;
        }
        return stats(s);
      },
    }),
    f,
  );
  f[0].source = "changed";
  f.reverse();
  hold.resolve();
  const r = await readBookInspection((await pending).bytes, original);
  assert.equal(r.chapters[0].path, "intro.md");
});
test("source findings preserve severity; accessibility findings are warnings without invented spans", async () => {
  const r = await run(
    engine({
      documentStats(s) {
        return {
          ...stats(s),
          findings: [
            { severity: "info", code: "unused_note", message: "Unused" },
            { severity: "error", code: "bad", message: "Broken" },
          ],
        };
      },
      accessibilityAudit() {
        return { ...audit(), findings: [{ code: "missing_alt", detail: "Image lacks alt text" }] };
      },
    }),
  );
  assert.deepEqual(r.summary.findings, { info: 2, warning: 2, error: 2 });
  assert.equal(r.summary.verdict, "findings");
  assert.equal(r.chapters[0].accessibility.findings[0].start, undefined);
});
test("failed stages never look clean and do not prevent later chapters or the other check", async () => {
  const r = await run(
    engine({
      documentStats(s) {
        if (s === source) throw "parser could not complete";
        return stats(s);
      },
      accessibilityAudit() {
        throw new Error("font unavailable");
      },
    }),
  );
  assert.equal(r.summary.verdict, "incomplete");
  assert.equal(r.summary.failedChecks, 3);
  assert.equal(r.summary.analyzedChapters, 1);
  assert.equal(r.summary.total.words, 5);
  assert.equal(r.chapters[0].statistics.message, "parser could not complete");
});
test("missing old-build APIs produce visible failed checks rather than fabricated empty findings", async () => {
  const r = await run({});
  assert.equal(r.summary.failedChecks, 4);
  assert.equal(r.summary.verdict, "incomplete");
  assert.equal(r.summary.total.words, 0);
});
test("engine schema, count, severity and source-length violations mark only that check incomplete", async () => {
  for (const transform of [
    (s) => ({ ...s, schema: "future" }),
    (s) => ({ ...s, bytes: s.bytes + 1 }),
    (s) => ({ ...s, words: -1 }),
    (s) => ({ ...s, words: NaN }),
    (s) => ({ ...s, findings: [{ severity: "fatal", code: "x", message: "x" }] }),
    (s) => ({ ...s, outline: [{ level: 7, text: "bad", slug: "bad" }] }),
    (s) => ({ ...s, findings: new Array(1) }),
  ]) {
    const r = await run(
      engine({
        documentStats(s) {
          return transform(stats(s));
        },
      }),
    );
    assert.equal(r.summary.failedChecks, 2);
    assert.equal(r.chapters[0].accessibility.status, "ok");
  }
});
test("oversized stages are explicit failures, not truncated clean reports", async () => {
  const r = await run(
    engine({
      documentStats(s) {
        return {
          ...stats(s),
          findings: Array.from({ length: 200 }, () => ({
            severity: "warning",
            code: "long",
            message: "x".repeat(1024),
          })),
        };
      },
    }),
  );
  assert.equal(r.chapters[0].statistics.status, "error");
  assert.equal(r.summary.verdict, "incomplete");
});
test("serialization excludes source bodies, image grants, audit text layer and unknown engine fields", async () => {
  const result = await inspectBook(engine(), files());
  const raw = new TextDecoder().decode(result.bytes);
  assert(!raw.includes("not retained"));
  assert(!raw.includes("A paragraph"));
  assert(!raw.includes("privateText"));
  const r = await readBookInspection(result.bytes, files());
  const downloaded = JSON.parse(await inspectionDownload(r).text());
  assert.deepEqual(downloaded, r);
  assert.equal(r.chapters[0].sourceSha256.length, 64);
});
test("fingerprints distinguish same-length source edits and exact BOM/CRLF variants", async () => {
  const result = await inspectBook(engine(), files());
  for (const newSource of [
    source.replace("Café", "Dafé"),
    source.replace("\r\n", "\n"),
    source.slice(1),
  ]) {
    const input = files();
    input[0].source = newSource;
    await assert.rejects(readBookInspection(result.bytes, input), { code: "STALE_INSPECTION" });
  }
});
test("changed chapter identity or order cannot receive another book report", async () => {
  const result = await inspectBook(engine(), files());
  for (const input of [files().reverse(), [{ path: "else.md", source }, files()[1]]])
    await assert.rejects(readBookInspection(result.bytes, input), { code: "STALE_INSPECTION" });
});
test("host recomputes verdict and totals instead of trusting wire claims", async () => {
  const result = parsed(
    await inspectBook(
      engine({
        accessibilityAudit() {
          throw "failed";
        },
      }),
      files(),
    ),
  );
  result.summary = { verdict: "no-findings", failedChecks: 0, total: { words: 999 } };
  const r = await readBookInspection(encode(result), files());
  assert.equal(r.summary.verdict, "incomplete");
  assert.equal(r.summary.total.words, 10);
});
test("malformed wire JSON, stages, hashes and encodings are rejected", async () => {
  const result = parsed(await inspectBook(engine(), files()));
  await assert.rejects(readBookInspection(new Uint8Array([0xff]), files()), {
    code: "INVALID_INSPECTION",
  });
  for (const changed of [
    { ...result, scope: "whole-book" },
    { ...result, chapters: [] },
    { ...result, chapters: [null, result.chapters[1]] },
  ])
    await assert.rejects(readBookInspection(encode(changed), files()));
  result.chapters[0].statistics.status = "skipped";
  await assert.rejects(readBookInspection(encode(result), files()), { code: "INVALID_INSPECTION" });
});
test("oversized input and invalid Unicode fail before invoking the engine", async () => {
  let calls = 0;
  const e = engine({
    documentStats() {
      calls++;
    },
  });
  for (const f of [
    [],
    new Array(1),
    [{ path: "a.md", source: "\ud800" }],
    [{ path: "a.md", source: "a".repeat(4 * 1024 * 1024 + 1) }],
    [files()[0], files()[0]],
    Array.from({ length: 129 }, (_, i) => ({ path: `${i}.md`, source: "" })),
  ])
    await assert.rejects(inspectBook(e, f));
  assert.equal(calls, 0);
});
test("bounded engine errors preserve astral text and repair only lone surrogate code units", async () => {
  const r = await run(
    engine({
      documentStats() {
        throw "🚀\ud800\ud801" + "x".repeat(600);
      },
    }),
  );
  assert(r.chapters[0].statistics.message.startsWith("🚀\ufffd\ufffd"));
  assert(Array.from(r.chapters[0].statistics.message).length <= 512);
});
test("engine-owned report objects cannot be mutated after inspection to change the result", async () => {
  const a = audit(),
    e = engine({
      accessibilityAudit() {
        return a;
      },
    });
  const result = await inspectBook(e, files());
  a.findings.push({ code: "later", detail: "mutated" });
  assert.equal((await readBookInspection(result.bytes, files())).summary.verdict, "no-findings");
});
test("ordinary long Unicode sources are counted as exact UTF-8 bytes", async () => {
  const f = [{ path: "unicode.md", source: "中🚀é\r\n".repeat(10000) }];
  const result = await inspectBook(engine(), f);
  assert.equal(result.sourceLength, new TextEncoder().encode(f[0].source).length);
  assert.equal(
    (await readBookInspection(result.bytes, f)).summary.total.bytes,
    result.sourceLength,
  );
});


test("explicit book inspection profiles reach both native analysis stages and every report surface", async () => {
  for (const profile of ["commonmark-gfm", "gfm-plus"]) {
    const calls = [], options = { profile };
    const e = engine({
      documentStats(s, analysis) {
        calls.push(["stats", s, analysis]);
        return { ...stats(s), profile: analysis.profile };
      },
      accessibilityAudit(s, analysis) {
        calls.push(["audit", s, analysis]);
        return { ...audit(), profile: analysis.profile };
      },
    });
    const output = await inspectBook(e, files(), options);
    const report = await readBookInspection(output.bytes, files(), options);
    assert.equal(output.profile, profile);
    assert.equal(report.profile, profile);
    assert.equal(parsed(output).profile, profile);
    assert.equal(JSON.parse(await inspectionDownload(report).text()).profile, profile);
    assert.deepEqual(calls.map(([kind, s]) => [kind, s]),
      [["stats", source], ["audit", source], ["stats", "# Next"], ["audit", "# Next"]]);
    for (const [, , analysis] of calls) {
      assert.deepEqual(analysis, { profile });
      assert(Object.isFrozen(analysis));
    }
    assert.equal(report.summary.verdict, "no-findings");
  }
});

test("omitted inspection profile preserves legacy native arguments and exact JSON layout", async () => {
  const arities = [];
  const output = await inspectBook(engine({
    documentStats(...args) { arities.push(args.length); return stats(args[0]); },
    accessibilityAudit(...args) { arities.push(args.length); return audit(); },
  }), files());
  const report = await readBookInspection(output.bytes, files());
  assert.deepEqual(arities, [1, 1, 1, 1]);
  assert(!Object.hasOwn(output, "profile"));
  assert(!Object.hasOwn(report, "profile"));
  assert.equal(new TextDecoder().decode(output.bytes),
    '{"schema":"fmd-book-inspection-v1","scope":"unexpanded-source-chapters","chapters":['
    + report.chapters.map(chapter => JSON.stringify(chapter)).join(",")
    + '],"summary":' + JSON.stringify(report.summary) + "}\n");
});

test("inspection captures the authoring profile and original source before asynchronous analysis", async () => {
  const f = files(), original = files(), options = { profile: "commonmark-gfm" };
  const hold = gate(), seen = [];
  const pending = inspectBook(engine({
    async documentStats(s, analysis) {
      seen.push([s, analysis.profile]);
      await hold.promise;
      return { ...stats(s), profile: analysis.profile };
    },
    accessibilityAudit(s, analysis) {
      seen.push([s, analysis.profile]);
      return { ...audit(), profile: analysis.profile };
    },
  }), f, options);
  options.profile = "gfm-plus";
  f[0].source = "changed"; f.reverse();
  hold.resolve();
  const result = await pending;
  const readingOptions = { profile: "commonmark-gfm" };
  const reading = readBookInspection(result.bytes, original, readingOptions);
  readingOptions.profile = "gfm-plus";
  const report = await reading;
  assert.equal(report.profile, "commonmark-gfm");
  assert.deepEqual(seen, [source, source, "# Next", "# Next"].map(s => [s, "commonmark-gfm"]));
});

test("explicit inspection refuses an old or wrong-dialect successful native stage", async () => {
  for (const stage of ["documentStats", "accessibilityAudit"]) {
    for (const receipt of [undefined, "gfm-plus", "future"]) {
      const e = engine({
        documentStats: s => ({ ...stats(s), profile: "commonmark-gfm" }),
        accessibilityAudit: () => ({ ...audit(), profile: "commonmark-gfm" }),
      });
      e[stage] = s => ({ ...(stage === "documentStats" ? stats(s) : audit()),
        ...(receipt === undefined ? {} : { profile: receipt }) });
      await assert.rejects(inspectBook(e, files(), { profile: "commonmark-gfm" }),
        { code: "UNSUPPORTED_WASM_PACKAGE" });
    }
  }
  await assert.rejects(inspectBook({}, files(), { profile: "gfm-plus" }),
    { code: "UNSUPPORTED_WASM_PACKAGE" });
});

test("profile capability errors refuse inspection while ordinary stage failures remain incomplete", async () => {
  const profile = "gfm-plus";
  await assert.rejects(inspectBook(engine({
    documentStats() {
      throw Object.assign(new Error("Rebuild profiled analysis"), { code: "UNSUPPORTED_WASM_PACKAGE" });
    },
  }), files(), { profile }), { code: "UNSUPPORTED_WASM_PACKAGE" });
  const result = await inspectBook(engine({
    documentStats() { throw new Error("Temporary analysis failure"); },
    accessibilityAudit: () => ({ ...audit(), profile }),
  }), files(), { profile });
  const report = await readBookInspection(result.bytes, files(), { profile });
  assert.equal(report.summary.failedChecks, 2);
  assert.equal(report.summary.verdict, "incomplete");
  assert.equal(report.chapters[0].accessibility.status, "ok");
});

test("same-source inspection identity includes the requested dialect and refuses unrequested receipts", async () => {
  const profile = "commonmark-gfm", options = { profile };
  const output = await inspectBook(engine({
    documentStats: s => ({ ...stats(s), profile }),
    accessibilityAudit: () => ({ ...audit(), profile }),
  }), files(), options);
  await assert.rejects(readBookInspection(output.bytes, files(), { profile: "gfm-plus" }),
    { code: "STALE_INSPECTION" });
  await assert.rejects(readBookInspection(output.bytes, files()), { code: "STALE_INSPECTION" });
  const raw = parsed(output);
  delete raw.profile;
  await assert.rejects(readBookInspection(encode(raw), files(), options), { code: "STALE_INSPECTION" });
  raw.profile = "future";
  await assert.rejects(readBookInspection(encode(raw), files(), options), { code: "STALE_INSPECTION" });
});

test("inspection admits only a data-valued dialect without reading source expansion or resource getters", async () => {
  const profile = "gfm-plus", options = { profile };
  for (const key of ["images", "fontAssets", "includeSources", "expandIncludes", "page",
    "typography", "title", "customCss", "fontEmbedding", "pdfA"])
    Object.defineProperty(options, key, { get() { throw new Error("Resource getter: " + key); } });
  const output = await inspectBook(engine({
    documentStats: s => ({ ...stats(s), profile }),
    accessibilityAudit: () => ({ ...audit(), profile }),
  }), files(), options);
  assert.equal((await readBookInspection(output.bytes, files(), options)).profile, profile);
  let reads = 0, calls = 0;
  for (const bad of [{ profile: "future" }, { get profile() { reads++; return profile; } }]) {
    await assert.rejects(inspectBook(engine({
      documentStats() { calls++; return stats(source); },
    }), files(), bad), { code: "INVALID_OPTIONS" });
  }
  assert.equal(reads, 0);
  assert.equal(calls, 0);
});

test("native profile receipts cannot hide behind getters", async () => {
  let reads = 0;
  const value = stats(source);
  Object.defineProperty(value, "profile", { get() { reads++; return "gfm-plus"; } });
  await assert.rejects(inspectBook(engine({ documentStats: () => value }), files(),
    { profile: "gfm-plus" }), { code: "UNSUPPORTED_WASM_PACKAGE" });
  assert.equal(reads, 0);
});
