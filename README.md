# LEDFX Editor

A browser editor for 96×16 LED-panel animations. The UI is React + TypeScript
(Vite); the Rust server owns USB.FXD/GIF import/export and BDF font parsing.
Everything between the browser and the server is JSON — there is no WebAssembly
build anymore.

The panel has an 8-colour hardware palette: black, red, green, yellow, blue,
magenta, cyan and white (indices 0–7). The index bits are BGR. Values 8–15 are
not displayable and are normalized to black on import, export and rendering.

## Architecture

| Path | Purpose |
|------|---------|
| `frontend/` | Vite + React + TypeScript editor |
| `src/model.rs` | Project/animation JSON model |
| `src/export.rs` | USB.FXD and GIF import/export (server-side) |
| `src/bdf.rs` | BDF font parser and text renderer (server-side) |
| `src/font.rs` | Shared JSON font model and BDF detection |
| `src/serve.rs` | Static file server + JSON API |

The browser edits and stores the project as JSON. When the user exports, the
animation JSON is posted to the server, which produces the binary USB.FXD or GIF
and returns it base64-encoded in a JSON response. The browser then downloads the
decoded bytes.

The project JSON format is the same as the old `.ledfx.json` format:

```json
{
  "name": "Untitled",
  "frames": [
    {
      "pixels": [0, 1, 2, "… 1536 entries total …"],
      "duration_ms": 100
    }
  ]
}
```

`pixels` is row-major: 16 rows × 96 columns, one palette index per entry. The
hardware palette uses indices 0–7 (black, red, green, yellow, blue, magenta,
cyan, white); indices 8–15 are normalized to black.

## Running

Requirements: a recent Rust toolchain and Node.js 20+.

Build the front-end:

```bash
cd frontend
npm install
npm run build
cd ..
```

Start the server (serves the built front-end and the API):

```bash
cargo run --release --bin serve -- 8080 frontend/dist
```

Open <http://localhost:8080>.

### Development

Run the backend and Vite separately. Vite proxies `/api` to port 8080:

```bash
# terminal 1
cargo run --bin serve -- 8080 frontend/dist

# terminal 2
cd frontend
npm run dev
```

Open <http://localhost:5173>.

## JSON API

All request and response bodies are JSON. Binary payloads are base64 strings.

| Method | Endpoint | Request | Response |
|--------|----------|---------|----------|
| `GET` | `/api/health` | — | `{ "ok": true, … }` |
| `POST` | `/api/import/usb-fxd` | `{ "data": "<base64 FXD>" }` | `{ "animation": { … } }` |
| `POST` | `/api/export/usb-fxd` | `{ "animation": { … } }` | `{ "filename", "mime_type", "data": "<base64 FXD>" }` |
| `POST` | `/api/import/gif` | `{ "data": "<base64 GIF>" }` | `{ "animation": { … } }` |
| `POST` | `/api/export/gif` | `{ "animation": { … } }` | `{ "filename", "mime_type", "data": "<base64 GIF>" }` |
| `POST` | `/api/fonts/parse` | `{ "data": "<base64 BDF/gzip>" }` | `{ "font": { "glyphs": { … }, … } }` |

Errors use an HTTP 4xx/5xx status and `{ "error": "message" }`.

The server accepts up to 65,535 USB.FXD frames. USB.FXD stores a single global
frame delay, so the exporter represents per-frame durations by repeating frames
using their common divisor when the expanded timeline fits in the slot limit;
otherwise it picks the delay that minimizes the total timing error. Files with
32 or fewer frames keep the historical 32-slot layout. Multi-program containers
are supported for import: the editor loads the first 96×16 program and
re-exports it as a single-program file.

## Tests

```bash
cargo test
cd frontend && npm run build
```
