// prole.jeffrey.build: the landing page with the current release filled in,
// and the DMGs, served from this site's own R2 bucket (prole-downloads).
import release from "../release.json" with { type: "json" };

const DMG_NAME = /^Prole-\d+\.\d+\.\d+\.dmg$/;

/** Rejects a release.json that can't describe a published DMG. It runs at
 * startup, so a malformed file fails `wrangler deploy` instead of going live. */
export function checkRelease(r) {
  const fail = (why) => {
    throw new Error(`release.json: ${why}`);
  };
  if (!/^\d+\.\d+\.\d+$/.test(r?.version ?? "")) fail(`version ${JSON.stringify(r?.version)} isn't x.y.z`);
  if (r.file !== `Prole-${r.version}.dmg`) fail(`file ${JSON.stringify(r.file)} should be Prole-${r.version}.dmg`);
  if (!/^[0-9a-f]{64}$/.test(r.sha256 ?? "")) fail("sha256 isn't 64 hex characters");
  if (!Number.isInteger(r.bytes) || r.bytes <= 0) fail("bytes isn't a positive integer");
  return r;
}
checkRelease(release);

export const dmgPath = (r = release) => `/download/${r.file}`;

export default {
  async fetch(request, env) {
    const url = new URL(request.url);
    const { pathname } = url;

    if (pathname === "/" || pathname === "/index.html") {
      return page(request, env, url);
    }
    // Old links and "always the newest": redirect, so the saved file still
    // carries its version.
    if (pathname === "/download/latest" || pathname === "/download/Prole.dmg") {
      return new Response(null, {
        status: 302,
        headers: { Location: new URL(dmgPath(), url).toString(), "Cache-Control": "no-cache" },
      });
    }
    if (pathname === "/download/latest.json") {
      return latestJson(url);
    }
    if (pathname.startsWith("/download/")) {
      let key;
      try {
        key = decodeURIComponent(pathname.slice("/download/".length));
      } catch {
        return notFound(env, request);
      }
      return download(request, env, key);
    }
    return env.ASSETS.fetch(request);
  },
};

async function download(request, env, key) {
  if (request.method !== "GET" && request.method !== "HEAD") {
    return new Response("Method not allowed", { status: 405, headers: { Allow: "GET, HEAD" } });
  }
  // Only published DMGs reach the bucket, never arbitrary keys.
  if (!DMG_NAME.test(key)) {
    return notFound(env, request);
  }

  const object =
    request.method === "HEAD"
      ? await env.DOWNLOADS.head(key)
      : await env.DOWNLOADS.get(key, { range: request.headers, onlyIf: request.headers });
  if (object === null) {
    return notFound(env, request);
  }

  const headers = new Headers();
  object.writeHttpMetadata(headers);
  headers.set("ETag", object.httpEtag);
  headers.set("Accept-Ranges", "bytes");
  headers.set("Content-Type", "application/x-apple-diskimage");
  headers.set("Content-Disposition", `attachment; filename="${key}"`);
  // A published version is never replaced, so its bytes never change.
  headers.set("Cache-Control", "public, max-age=31536000, immutable");

  if (request.method === "HEAD") {
    headers.set("Content-Length", String(object.size));
    return new Response(null, { headers });
  }
  // `onlyIf` didn't match (e.g. If-None-Match hit): R2 returns no body.
  if (!("body" in object)) {
    return new Response(null, { status: 304, headers });
  }
  // R2 reports a range even for a plain GET; only a requested range is a 206.
  if (request.headers.has("Range") && object.range) {
    const { offset, length } = rangeOf(object.range, object.size);
    headers.set("Content-Range", `bytes ${offset}-${offset + length - 1}/${object.size}`);
    headers.set("Content-Length", String(length));
    return new Response(object.body, { status: 206, headers });
  }
  headers.set("Content-Length", String(object.size));
  return new Response(object.body, { headers });
}

/** R2 reports `{offset, length}` or `{suffix}`; normalise to offset/length.
 * The object can carry `suffix: undefined`, so test the value, not the key. */
export function rangeOf(range, size) {
  if (typeof range.suffix === "number") {
    const length = Math.min(range.suffix, size);
    return { offset: size - length, length };
  }
  const offset = range.offset ?? 0;
  return { offset, length: range.length ?? size - offset };
}

function latestJson(url) {
  const body = { ...release, url: `${publicOrigin(url)}${dmgPath()}` };
  return new Response(`${JSON.stringify(body, null, 2)}\n`, {
    headers: { "Content-Type": "application/json; charset=utf-8", "Cache-Control": "no-cache" },
  });
}

/** The site's 404 page, with a 404 status. Any path the assets don't have
 * gets it (not_found_handling: "404-page"). */
async function notFound(env, request) {
  const missing = await env.ASSETS.fetch(new Request(new URL("/.not-found", request.url)));
  return new Response(missing.body, { status: 404, headers: missing.headers });
}

/** Downloads are always https, except when developing locally. */
export function publicOrigin(url) {
  return url.hostname === "localhost" || url.hostname === "127.0.0.1" ? url.origin : `https://${url.host}`;
}

async function page(request, env, url) {
  // "/" (not "/index.html", which the asset handler redirects to "/").
  const asset = await env.ASSETS.fetch(new Request(new URL("/", url), request));
  if (!asset.ok || !asset.headers.get("Content-Type")?.includes("text/html")) {
    return asset;
  }
  const rewriter = pageHandlers(release, publicOrigin(url)).reduce(
    (rw, [selector, handler]) => rw.on(selector, handler),
    new HTMLRewriter(),
  );
  const response = rewriter.transform(asset);
  const headers = new Headers(response.headers);
  headers.set("Cache-Control", "public, max-age=300");
  return new Response(response.body, { status: response.status, headers });
}

/** What the page gets from the release: every download link, the file name,
 * the version label and the structured data. Fresh per request (the JSON-LD
 * handler buffers text). */
export function pageHandlers(r, origin) {
  let jsonLd = "";
  return [
    ["[data-download]", { element: (el) => el.setAttribute("href", dmgPath(r)) }],
    ["[data-file-name]", { element: (el) => el.setInnerContent(r.file) }],
    ["[data-version]", { element: (el) => el.setInnerContent(`v${r.version}`) }],
    [
      "script#app-ld",
      {
        // Script text can arrive in several chunks; rewrite it once it's whole.
        text(chunk) {
          jsonLd += chunk.text;
          if (!chunk.lastInTextNode) {
            chunk.remove();
            return;
          }
          chunk.replace(updateJsonLd(jsonLd, r, origin), { html: true });
          jsonLd = "";
        },
      },
    ],
  ];
}

/** Structured data with the current version and download URL. Leaves the
 * text alone if it isn't JSON; escapes "<" so it can't close the script. */
export function updateJsonLd(text, r, origin) {
  let data;
  try {
    data = JSON.parse(text);
  } catch {
    return text;
  }
  data.softwareVersion = r.version;
  data.downloadUrl = `${origin}${dmgPath(r)}`;
  return `\n${JSON.stringify(data, null, 2).replace(/</g, "\\u003c")}\n    `;
}
