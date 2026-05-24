# Deploying try.slab.app

`try.slab.app` is the in-browser playground for Slab — the same SvelteKit
SPA that ships inside the Tauri desktop app, deployed standalone to
Cloudflare Pages so anyone can try the PDF playground without an install.

The build runs in CI on every push to `main` that touches front-end
sources, via [`.github/workflows/deploy-try.yml`](../../.github/workflows/deploy-try.yml).

This document is the one-time setup checklist for **Sanjay** to perform
in the Cloudflare dashboard and GitHub repo settings. Until these steps
are done, the workflow runs in dry-run mode (builds the SPA, uploads it
as a CI artifact for inspection, but skips the deploy step).

---

## 1. Create the Cloudflare Pages project (one-time)

1. Go to [Cloudflare dashboard → Workers & Pages → Create application →
   Pages → Direct Upload](https://dash.cloudflare.com/?to=/:account/workers-and-pages/create/pages).
2. Project name: **`slab-try`** (must match the `--project-name` flag in
   the workflow exactly).
3. Production branch: `main`.
4. Skip "connect to Git" — we deploy via the GitHub Action, not Cloudflare's
   native Git integration (gives us full control over the build).

## 2. Generate a scoped API token

1. Cloudflare dashboard → **My Profile → API Tokens → Create Token**.
2. Use the **"Edit Cloudflare Workers"** template, then customize:
   - **Permissions:**
     - `Account` → `Cloudflare Pages` → `Edit`
     - `Account` → `Account Settings` → `Read`
   - **Account resources:** restrict to your Slab account.
   - **TTL:** none (or set a calendar reminder to rotate annually).
3. Copy the token — it's shown once.

## 3. Find your Cloudflare account ID

Cloudflare dashboard → **Workers & Pages** → right sidebar shows
**Account ID** (32-char hex).

## 4. Add GitHub secrets

GitHub repo → **Settings → Secrets and variables → Actions → New repository
secret**:

| Secret name              | Value                                    |
| ------------------------ | ---------------------------------------- |
| `CLOUDFLARE_API_TOKEN`   | the token from step 2                    |
| `CLOUDFLARE_ACCOUNT_ID`  | the account ID from step 3               |

## 5. Custom domain (try.slab.app)

In the Cloudflare Pages project (`slab-try`):

1. **Custom domains → Set up a custom domain → `try.slab.app`**.
2. Cloudflare auto-provisions a TLS cert and creates the CNAME record
   inside the `slab.app` zone (assuming the zone is in the same Cloudflare
   account — which it should be).
3. Within ~60 seconds, `https://try.slab.app/` should serve the playground
   and redirect to `/try/` per the `_redirects` file the workflow writes.

If the `slab.app` zone lives in a different registrar / account, manually
add a CNAME:

```
try   CNAME   slab-try.pages.dev   proxied=true
```

## 6. Trigger the first deploy

Once all of the above is in place, fire the workflow manually:

- GitHub → Actions → **deploy-try** → **Run workflow** → Branch: `main`.

After that, every push to `main` that touches `src/**`, `static/**`, or
the build config will redeploy automatically.

## 7. Verify

- `https://try.slab.app/` → 301 → `/try/`
- `https://try.slab.app/try/` → playground landing
- `https://try.slab.app/try/pages` → page-ops tool
- `https://try.slab.app/try/markdown` → live Markdown→PDF editor
- `https://try.slab.app/try/metadata` → metadata editor
- All routes should work fully offline after first load (SPA shell).

## 8. Rolling back

`slab-try` Pages project → **Deployments** → pick a prior deployment →
**Rollback to this deployment**. Takes ~10 seconds.

---

## Local preview

To preview the production build locally without Cloudflare:

```bash
pnpm install
pnpm build
pnpm preview            # serves on http://localhost:4173
# Open http://localhost:4173/try/
```

## Privacy guarantee

The playground is 100% client-side — no PDFs leave the browser, no
analytics beacons, no third-party scripts. The privacy banner on `/try/`
(`<PrivacyBanner>`) reflects this. If you ever add server-side anything,
update that banner first.
