# Prole site and downloads on Cloudflare

**Date:** 2026-10-28 · **Status:** approved design · **bd:** `ai-agents-pej.3` (epic `ai-agents-pej`)

## Context

The Hetzner account is being shut down. Today `prole.jeffrey.build` is an nginx container on
Hetzner/Coolify (built from `ai-agents/apps/prole-site`), and DMGs are uploaded over SSH into a
bind-mounted folder. `jeffrey.build` DNS already moved to Cloudflare on 2026-10-28; every record is
still DNS-only and points at Hetzner.

This design moves the Prole landing page and its downloads onto a Cloudflare Worker with its own R2
bucket. Nothing is shared with any other project's Worker, bucket or domain.

**Goals**

- `prole.jeffrey.build` serves the page and every DMG from Cloudflare; nothing depends on Hetzner.
- Download URLs carry the version (`/download/Prole-<version>.dmg`); published versions never change.
- One command (`scripts/release.sh`) builds, verifies, publishes and updates the page.

**Non-goals:** notarization, an update feed inside the app, analytics, migrating the other
`jeffrey.build` services (separate tasks in the same epic).

## Architecture

```
prole/
  site/
    wrangler.jsonc        Worker config: assets, R2 binding, custom domain
    package.json          wrangler (dev only); tests use node:test
    release.json          the current release (written by release.sh, committed)
    src/worker.js         routes below
    test/worker.test.js   route tests with fake ASSETS/R2
    public/               index.html, styles.css, assets/, robots.txt, sitemap.xml, llms.txt, 404.html
```

- **Worker** `prole-site`, custom domain `prole.jeffrey.build`, `workers_dev: false` once live.
- **R2 bucket** `prole-downloads` (binding `DOWNLOADS`), holding only `Prole-<x.y.z>.dmg` objects.
- **Static assets** from `site/public`; the Worker runs first only for `/`, `/index.html` and
  `/download/*`.

### `release.json`

```json
{ "version": "0.2.0", "file": "Prole-0.2.0.dmg", "sha256": "…", "bytes": 8814107,
  "commit": "b628a15", "date": "2026-10-28" }
```

`release.sh` writes it; it's bundled into the Worker at deploy time. `version` must be `x.y.z` and
`file` must be `Prole-<version>.dmg`; a check at deploy time and a unit test reject anything else.

### Routes

| Path | Response |
| --- | --- |
| `/`, `/index.html` | The page, with the release filled in by `HTMLRewriter`: `[data-download]` links → `/download/<file>`, `[data-file-name]` text → file name, `[data-version]` text → `v<version>`, JSON-LD `#app-ld` `softwareVersion`/`downloadUrl`. No client-side script. |
| `/download/Prole-<x.y.z>.dmg` | Streamed from R2 with `Range`/`If-None-Match` support, `Content-Type: application/x-apple-diskimage`, `Content-Disposition: attachment; filename="<file>"`, `Cache-Control: public, max-age=31536000, immutable`. |
| `/download/Prole.dmg`, `/download/latest` | `302` to `/download/<current file>` (old links keep working and still save a versioned file name). |
| `/download/latest.json` | `release.json` plus an absolute `url`, `Cache-Control: no-cache`. |
| anything else under `/download/` | `404` (site 404 page). Only names matching `^Prole-\d+\.\d+\.\d+\.dmg$` reach R2. |
| everything else | Static assets. |

Downloads accept `GET` and `HEAD` only (`405` otherwise). A name that matches but isn't in R2 is a
`404`.

## Release flow (`scripts/release.sh`)

Building and verifying are unchanged: build in `~/Library/Caches/prole-release` (outside iCloud),
copy without extended attributes, re-sign ad-hoc, strict-verify, make the DMG with `hdiutil`,
mount it and check signature, Applications link and both architectures. Then:

1. **Preflight** (before building): clean git tree; `package.json`, `Cargo.toml` and
   `tauri.conf.json` agree on the version; `wrangler whoami` succeeds; the version is not already in
   `site/release.json` (otherwise: bump the version).
2. **Upload** the DMG: `wrangler r2 object put prole-downloads/Prole-<version>.dmg --remote`.
3. **Write** `site/release.json`, then `wrangler deploy` from `site/`.
4. **Commit** `site/release.json` as `chore(release): Prole <version>` (local commit; push as usual).
   A version is "published" once it is in `site/release.json`; the preflight refuses it (no network
   check, so an interrupted run can simply be repeated).
5. **Verify over HTTPS:** versioned URL's sha256 equals the build's; `/download/Prole.dmg` is a
   `302` to it; `latest.json` reports the version; the page links the versioned file.

The SSH/Hetzner upload path, `--replace` and the server-side archive are removed.

**Rollback a release:** revert the release commit and `wrangler deploy`; the previous DMG is still in
R2 (objects are never overwritten).

## Cut-over

1. Create `prole-downloads`; upload `Prole-0.2.0.dmg` and check its sha256 matches the live file
   (`01d5ea83…a11d`).
2. Add `site/` (page ported from `ai-agents/apps/prole-site`, client script removed) and its tests.
3. Deploy with `workers_dev: true` and test every route on the `*.workers.dev` URL.
4. Delete the `prole` A record in Cloudflare DNS, add the custom domain `prole.jeffrey.build` to the
   Worker (brief gap while the certificate is issued), set `workers_dev: false`, redeploy.
5. Run the release verification checks against `prole.jeffrey.build`.

**Rollback during cut-over (while Hetzner still exists):** remove the custom domain and restore the
`prole` A record → `95.217.185.93`. After Hetzner is deleted there is no Hetzner fallback; rollback
means redeploying an earlier Worker version.

## Testing

- `site/test/worker.test.js` (node:test): page rewriting, each download route and status code,
  redirect targets, `latest.json`, name filtering, `405`, missing object `404`, `release.json`
  validation, `Range` responses.
- `release.sh --no-upload` builds and verifies without publishing.
- Post-deploy HTTPS checks (step 5 of the release flow) on every release.

## Cleanup

- Merge PR #3 first (0.2.0 version bump and versioned naming), then this work replaces its upload.
- Delete `ai-agents/apps/prole-site` and stop `prole-site` in Coolify once the Worker is live.
- Older DMGs (`2026-06-12`, two unversioned 2026-10-28 builds) stay in the local backup
  `~/hetzner-backup-2026-10-28/prole-releases/`; they are not republished.
- Update the landing-page memory/notes: no SSH, no Coolify for Prole.
