import { test } from "node:test";
import assert from "node:assert/strict";
import release from "../release.json" with { type: "json" };
import worker, { checkRelease, pageHandlers, rangeOf, updateJsonLd } from "../src/worker.js";

const ORIGIN = "https://prole.jeffrey.build";
const DMG = release.file;
const BYTES = new TextEncoder().encode("0123456789");

/** ASSETS that records what it was asked for; "/.not-found" is the 404 page. */
function fakeEnv({ objects = { [DMG]: BYTES } } = {}) {
  const assetPaths = [];
  const object = (key, extra = {}) => ({
    size: objects[key].length,
    httpEtag: '"etag-1"',
    writeHttpMetadata: () => {},
    ...extra,
  });
  return {
    assetPaths,
    ASSETS: {
      async fetch(req) {
        const { pathname } = new URL(req.url);
        assetPaths.push(pathname);
        const missing = pathname === "/.not-found";
        return new Response(missing ? "not found page" : `asset ${pathname}`, {
          status: missing ? 404 : 200,
          headers: { "Content-Type": "text/html" },
        });
      },
    },
    DOWNLOADS: {
      async head(key) {
        return key in objects ? object(key) : null;
      },
      async get(key, { range, onlyIf }) {
        if (!(key in objects)) return null;
        if (onlyIf.get("If-None-Match") === '"etag-1"') return object(key); // no body
        const bytes = objects[key];
        const m = /^bytes=(\d+)-(\d+)$/.exec(range.get("Range") ?? "");
        if (m) {
          const [offset, end] = [Number(m[1]), Number(m[2])];
          return object(key, { body: new Blob([bytes.slice(offset, end + 1)]).stream(), range: { offset, length: end - offset + 1 } });
        }
        return object(key, { body: new Blob([bytes]).stream() });
      },
    },
  };
}

const call = (path, init, env = fakeEnv()) => worker.fetch(new Request(`${ORIGIN}${path}`, init), env);

test("release.json is valid and matches the published file name", () => {
  assert.equal(checkRelease(release), release);
  assert.equal(release.file, `Prole-${release.version}.dmg`);
});

test("a malformed release fails at startup", () => {
  const ok = { ...release };
  assert.throws(() => checkRelease({ ...ok, version: "0.2" }), /x\.y\.z/);
  assert.throws(() => checkRelease({ ...ok, file: "Prole.dmg" }), /should be/);
  assert.throws(() => checkRelease({ ...ok, sha256: "abc" }), /sha256/);
  assert.throws(() => checkRelease({ ...ok, bytes: 0 }), /bytes/);
});

test("old and 'latest' links redirect to the versioned file", async () => {
  for (const path of ["/download/latest", "/download/Prole.dmg"]) {
    const res = await call(path);
    assert.equal(res.status, 302, path);
    assert.equal(res.headers.get("Location"), `${ORIGIN}/download/${DMG}`);
    assert.equal(res.headers.get("Cache-Control"), "no-cache");
  }
});

test("latest.json describes the current release with an absolute URL", async () => {
  const res = await call("/download/latest.json");
  assert.equal(res.status, 200);
  assert.match(res.headers.get("Content-Type"), /application\/json/);
  assert.equal(res.headers.get("Cache-Control"), "no-cache");
  const body = await res.json();
  assert.equal(body.version, release.version);
  assert.equal(body.sha256, release.sha256);
  assert.equal(body.url, `${ORIGIN}/download/${DMG}`);
});

test("the DMG streams from R2 as an immutable, versioned attachment", async () => {
  const res = await call(`/download/${DMG}`);
  assert.equal(res.status, 200);
  assert.equal(res.headers.get("Content-Type"), "application/x-apple-diskimage");
  assert.equal(res.headers.get("Content-Disposition"), `attachment; filename="${DMG}"`);
  assert.equal(res.headers.get("Cache-Control"), "public, max-age=31536000, immutable");
  assert.equal(res.headers.get("Content-Length"), "10");
  assert.equal(await res.text(), "0123456789");
});

test("HEAD gives the size without a body", async () => {
  const res = await call(`/download/${DMG}`, { method: "HEAD" });
  assert.equal(res.status, 200);
  assert.equal(res.headers.get("Content-Length"), "10");
  assert.equal(await res.text(), "");
});

test("range requests resume a download", async () => {
  const res = await call(`/download/${DMG}`, { headers: { Range: "bytes=2-5" } });
  assert.equal(res.status, 206);
  assert.equal(res.headers.get("Content-Range"), "bytes 2-5/10");
  assert.equal(res.headers.get("Content-Length"), "4");
  assert.equal(await res.text(), "2345");
});

test("a matching ETag is a 304", async () => {
  const res = await call(`/download/${DMG}`, { headers: { "If-None-Match": '"etag-1"' } });
  assert.equal(res.status, 304);
});

test("only published DMG names reach the bucket", async () => {
  for (const path of ["/download/secrets.txt", "/download/Prole-1.dmg", "/download/..%2Fwrangler.jsonc", "/download/%E0%A4%A"]) {
    const env = fakeEnv();
    const res = await call(path, undefined, env);
    assert.equal(res.status, 404, path);
    assert.equal(await res.text(), "not found page");
  }
});

test("a well-formed name that isn't in R2 is a 404", async () => {
  const res = await call("/download/Prole-9.9.9.dmg");
  assert.equal(res.status, 404);
});

test("downloads accept only GET and HEAD", async () => {
  const res = await call(`/download/${DMG}`, { method: "POST" });
  assert.equal(res.status, 405);
  assert.equal(res.headers.get("Allow"), "GET, HEAD");
});

test("other paths are plain static assets", async () => {
  const env = fakeEnv();
  const res = await call("/styles.css", undefined, env);
  assert.equal(await res.text(), "asset /styles.css");
});

test("rangeOf handles suffix and open-ended ranges", () => {
  assert.deepEqual(rangeOf({ suffix: 3 }, 10), { offset: 7, length: 3 });
  assert.deepEqual(rangeOf({ offset: 4 }, 10), { offset: 4, length: 6 });
  // Shape R2 actually returns (seen under wrangler dev): suffix key present but undefined.
  assert.deepEqual(rangeOf({ offset: 0, length: 100, suffix: undefined }, 1000), { offset: 0, length: 100 });
});

test("a plain GET is a 200 even when R2 reports a full range", async () => {
  const env = fakeEnv();
  const get = env.DOWNLOADS.get;
  env.DOWNLOADS.get = async (key, opts) => ({ ...(await get(key, opts)), range: { offset: 0, length: 10, suffix: undefined } });
  const res = await call(`/download/${DMG}`, undefined, env);
  assert.equal(res.status, 200);
  assert.equal(res.headers.get("Content-Range"), null);
  assert.equal(res.headers.get("Content-Length"), "10");
});

test("the page links, labels and structured data get the release", () => {
  const r = { ...release, version: "1.2.3", file: "Prole-1.2.3.dmg" };
  const [link, fileName, version, ld] = pageHandlers(r, ORIGIN).map(([, h]) => h);
  const el = { attrs: {}, setAttribute(k, v) { this.attrs[k] = v; }, setInnerContent(v) { this.content = v; } };
  link.element(el);
  fileName.element(el);
  assert.equal(el.attrs.href, "/download/Prole-1.2.3.dmg");
  assert.equal(el.content, "Prole-1.2.3.dmg");
  version.element(el);
  assert.equal(el.content, "v1.2.3");

  // JSON-LD arriving in two chunks is rewritten once, whole.
  const out = [];
  const chunk = (text, last) => ({ text, lastInTextNode: last, remove: () => out.push("<removed>"), replace: (t) => out.push(t) });
  ld.text(chunk('{"softwareVersion":"0.1.0",', false));
  ld.text(chunk('"downloadUrl":"x","name":"Prole"}', true));
  assert.equal(out[0], "<removed>");
  const data = JSON.parse(out[1]);
  assert.equal(data.softwareVersion, "1.2.3");
  assert.equal(data.downloadUrl, `${ORIGIN}/download/Prole-1.2.3.dmg`);
  assert.equal(data.name, "Prole");
});

test("structured data can't break out of its script tag", () => {
  const out = updateJsonLd('{"description":"</script><b>"}', release, ORIGIN);
  assert.ok(!out.includes("</script>"));
  assert.equal(JSON.parse(out).description, "</script><b>");
  assert.equal(updateJsonLd("not json", release, ORIGIN), "not json");
});
