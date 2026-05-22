# Landing demos

This directory holds the public-facing animated demos used on the landing page
(`docs/landing/index.html`).

## What's here

- `hero.svg`        — 12s loop: open → highlight → sign → save
- `merge.svg`       — 3s loop: merge two PDFs into one
- `edit.svg`        — 4s loop: inline text edit, layout preserved
- `sign.svg`        — 4s loop: sign + flatten to locked PDF

All four are **pure SVG + CSS keyframes**. No external assets, no JavaScript,
no CDN dependencies. Each file is under 7 KB. They respect
`prefers-reduced-motion: reduce` automatically.

## Why SVG, not video?

1. **Trust matches the product.** Slab's pitch is "your files never leave the
   machine." Shipping a 5 MB MP4 demo that hits a CDN undermines that.
2. **Total weight under 18 KB** for all four animations vs. ~6 MB for the
   video bundle #27 originally requested.
3. **Crisp at any DPI** — important for the macOS/Windows/Linux audience
   that includes 4K displays.
4. **Lighthouse score stays at 100** with no LCP impact.

## When to swap in real video

If/when a real recorded demo is needed (e.g. for a YouTube ad or App Store
listing), drop captures into `../../static/demos/` as `hero.mp4`,
`merge.mp4`, etc., and swap the `<object data="demos/foo.svg">` tags in
`index.html` for:

```html
<video src="/demos/foo.mp4" autoplay loop muted playsinline></video>
```

Recording recipe (requires the running app, so this is a manual step):

1. `pnpm tauri dev` to launch Slab.
2. `quicktime → File → New Screen Recording → Selection`.
3. Capture at 1600×1000, trim to the timings listed above.
4. Encode: `ffmpeg -i raw.mov -vf scale=800:500 -c:v libx264 -crf 28 -an out.mp4`.
5. Target: each clip under 1.5 MB, all four under 6 MB total.
