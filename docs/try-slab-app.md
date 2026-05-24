# Deploying `try.slab.app`

The `/try` route is part of the same SvelteKit build that ships in the
Tauri desktop bundle. To deploy it as a standalone static site at
`https://try.slab.app`, follow this guide.

## Build

```bash
pnpm install
pnpm build
```

The output is in `build/`. Because `svelte.config.js` uses
`adapter-static` with `fallback: 'index.html'`, the bundle is a single
static SPA that hydrates client-side. The `/try` subtree forces
`prerender = true; ssr = false;` so it works under any static host.

## Deploy targets

### Cloudflare Pages (recommended)

1. Create a new Pages project, point it at the repo.
2. Build command: `pnpm install --frozen-lockfile && pnpm build`
3. Build output directory: `build`
4. Add a custom domain: `try.slab.app`.
5. Optional: set the project root to redirect `/` to `/try` so the
   bare domain lands on the playground.

### Vercel

1. Import the repo.
2. Framework preset: SvelteKit.
3. Domain: `try.slab.app`.

### Static S3 / Caddy / nginx

Serve `build/` as the document root. Ensure 200-on-not-found falls back
to `index.html` so SPA routing works.

## DNS

```
try    CNAME    <cloudflare-or-vercel-target>
```

## CSP (recommended hardening)

Once the playground is live, set the following response header on
`/try/*`:

```
Content-Security-Policy: default-src 'self'; img-src 'self' data: blob:; worker-src 'self' blob:; script-src 'self' 'wasm-unsafe-eval';
```

`'wasm-unsafe-eval'` is needed for `pdf-lib` / `pdfjs-dist`. No
`connect-src` allowance to third parties — that's the wedge.

## Sample PDFs

Samples are minted at `scripts/mint-samples.mjs` and written to
`static/try/samples/`. They are deterministic (fixed creation date) so
they don't churn the git history. To re-mint:

```bash
node scripts/mint-samples.mjs
```

## Privacy banner

The `<PrivacyBanner />` component uses `PerformanceObserver` to count
bytes transferred to cross-origin endpoints. On a healthy deployment
this number must stay at **0** for any user-initiated action.

If you fork and add analytics, that counter will start ticking and the
trust signal evaporates — please don't.
