# Slab Server — Self-Hosted PDF Toolkit 🐳

Slab v2.1.0 introduces **Slab Server**: the same Rust PDF core that powers the
desktop app, exposed over an HTTP API and a drag-drop web UI. Run it on your
NAS, in a Docker Compose stack, in a Kubernetes cluster, or in front of a
reverse proxy on your homelab — your PDFs never leave your machine.

> **Slab Server is opt-in.** It is published as a separate Docker image
> (`ghcr.io/sanjays2402/slab`). The desktop installers on the
> [releases page](https://github.com/Sanjays2402/slab/releases)
> do **not** include the server, and the server image does **not** include
> the desktop GUI.

## Quickstart

### Docker

```bash
docker run --rm -p 8080:8080 ghcr.io/sanjays2402/slab:latest
```

Open <http://localhost:8080> and drop a PDF on the page.

### Docker Compose

```yaml
services:
  slab:
    image: ghcr.io/sanjays2402/slab:latest
    ports: ["8080:8080"]
    restart: unless-stopped
    volumes:
      - slab_data:/var/lib/slab
volumes:
  slab_data: {}
```

The repo's [`docker-compose.yml`](../docker-compose.yml) is a hardened
version of the above (read-only root FS, dropped Linux capabilities,
no-new-privileges, healthcheck wired up).

### From source

```bash
cd src-tauri
cargo run --release --bin slab-server --features server
```

## Configuration

Everything is environment-variable based. Defaults in **bold**.

| Variable             | Default              | Notes                                                                       |
| -------------------- | -------------------- | --------------------------------------------------------------------------- |
| `SLAB_BIND`          | **`0.0.0.0:8080`**   | `host:port` to listen on. Use `127.0.0.1:8080` to restrict to localhost.    |
| `SLAB_MAX_UPLOAD_MB` | **`256`**            | Per-request multipart cap. Bump for very large PDFs.                        |
| `SLAB_DATA_DIR`      | **`/var/lib/slab`**  | Persistent dir for future job history & embedding cache.                    |
| `SLAB_CORS_ORIGINS`  | (empty — same-origin)| Comma-separated list of allowed origins. Set to `*` to disable.             |
| `RUST_LOG`           | **`info`**           | `tracing-subscriber` env filter (`debug`, `slab_server=trace`, etc.).       |

## HTTP API

Slab Server speaks JSON for metadata and `application/pdf` (or
`application/zip` for multi-output operations) for the file responses.
Every operation is a `POST` with `multipart/form-data` — drop the PDF
under the field name `file`, add params alongside.

The full op index is also reachable at **`GET /api/v1/ops`** so you can
generate clients from a live deployment.

### Endpoints

| Method | Path                          | Body fields                        | Response               |
| ------ | ----------------------------- | ---------------------------------- | ---------------------- |
| GET    | `/healthz`                    | —                                  | `200` `application/json` |
| GET    | `/api/v1/ops`                 | —                                  | Op index               |
| POST   | `/api/v1/merge`               | `file` (×N, ordered)               | merged PDF             |
| POST   | `/api/v1/split-every`         | `file`, `chunk_size`               | zip of PDFs            |
| POST   | `/api/v1/split-ranges`        | `file`, `ranges` (`1-3,5,7-9`)     | zip of PDFs            |
| POST   | `/api/v1/rotate`              | `file`, `pages`, `degrees`         | rotated PDF            |
| POST   | `/api/v1/delete-pages`        | `file`, `pages`                    | trimmed PDF            |
| POST   | `/api/v1/reorder-pages`       | `file`, `order`                    | reordered PDF          |
| POST   | `/api/v1/compress`            | `file`                             | compressed PDF (+`x-slab-bytes-before/after`) |
| POST   | `/api/v1/encrypt`             | `file`, `password`                 | encrypted PDF          |
| POST   | `/api/v1/decrypt`             | `file`, `password`                 | decrypted PDF          |
| POST   | `/api/v1/watermark`           | `file`, `text`, `opacity` (0–1)    | watermarked PDF        |
| POST   | `/api/v1/extract-text`        | `file`                             | `text/plain`           |
| POST   | `/api/v1/info`                | `file`                             | JSON `PdfInfo`         |
| POST   | `/api/v1/page-count`          | `file`                             | `{ "pages": N }`       |
| POST   | `/api/v1/strip-metadata`      | `file`                             | sanitized PDF          |

### Curl examples

```bash
# Health check
curl http://localhost:8080/healthz

# Merge two PDFs (order matters)
curl -F file=@first.pdf -F file=@second.pdf \
  -o merged.pdf http://localhost:8080/api/v1/merge

# Compress with savings reported in response headers
curl -F file=@big.pdf -D - -o small.pdf \
  http://localhost:8080/api/v1/compress | grep -i x-slab-bytes

# Encrypt
curl -F file=@secret.pdf -F password=hunter2 \
  -o locked.pdf http://localhost:8080/api/v1/encrypt

# Watermark with custom opacity
curl -F file=@in.pdf -F text=CONFIDENTIAL -F opacity=0.25 \
  -o stamped.pdf http://localhost:8080/api/v1/watermark

# Get structured metadata
curl -F file=@in.pdf http://localhost:8080/api/v1/info | jq

# Split into 3-page chunks
curl -F file=@in.pdf -F chunk_size=3 \
  -o chunks.zip http://localhost:8080/api/v1/split-every

# Split by ranges 1–3, 5, 7–9
curl -F file=@in.pdf -F 'ranges=1-3,5,7-9' \
  -o ranges.zip http://localhost:8080/api/v1/split-ranges
```

### Error format

Errors are always JSON, with a stable shape:

```json
{
  "error": "watermark text is empty",
  "code": "bad_request"
}
```

`code` is one of `bad_request`, `not_found`, `payload_too_large`,
`unsupported_media_type`, or `internal`.

## Security model

Slab Server is **not** intended to be exposed directly to the public
internet. It has no authentication, no rate limiting, and no per-tenant
isolation. Run it behind:

- a reverse proxy with HTTP basic auth or an OIDC sidecar (Caddy, Traefik,
  Authelia, oauth2-proxy), **or**
- on a trusted network segment (homelab VLAN, VPN, Tailscale tailnet).

All PDF processing happens in-process. The container has no outbound
network requirements; you can run it with `--network=none` if you don't
need Beacon AI features.

## Why not just run the desktop app?

The desktop app needs a GUI runtime (WebKit on macOS/Linux, WebView2 on
Windows). It's the right answer for laptops; it's the wrong answer for:

- **Headless servers** — no display server, no point in WebKit.
- **CI pipelines** — `docker run` is one line, no `xvfb-run` chains.
- **Shared homelab tools** — one container, many household users.
- **Programmatic batch jobs** — drive the API from any language.

Slab Server is the same Rust crate underneath. Bugs fixed in one are
fixed in the other.

## What's missing in v2.1.0

These are deliberate non-goals for the initial release and are tracked
for v2.2.0+:

- **OCR** — needs Tesseract bundled into the image (+150 MB). Coming as
  an opt-in `ghcr.io/sanjays2402/slab:ocr` variant.
- **`linux/arm64` image** — held back until a self-hosted aarch64
  runner lands (cross-compile via QEMU is ~12 min per arch).
- **AuthN/AuthZ** — see the security note above; the right answer is a
  reverse proxy, not an in-binary password.
- **Persistent job queue** — currently every request is synchronous.
  Long-running ops (large compress) hold the connection open.

If any of these block you, open an issue: <https://github.com/Sanjays2402/slab/issues>.
