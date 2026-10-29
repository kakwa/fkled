# LEDFX Editor

A browser editor for 96×16 LED-panel animations. The UI is React + TypeScript
(Vite); the Rust server owns USB.FXD/GIF import/export and PCF font parsing.
Everything between the browser and the server is JSON — there is no WebAssembly
build anymore.

## Architecture

| Path | Purpose |
|------|---------|
| `frontend/` | Vite + React + TypeScript editor |
| `src/model.rs` | Project/animation JSON model |
| `src/export.rs` | USB.FXD and GIF import/export (server-side) |
| `src/pcf.rs` | PCF font parser (server-side) |
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

`pixels` is row-major: 16 rows × 96 columns, one palette index (0–15) per entry.

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
| `POST` | `/api/fonts/parse` | `{ "data": "<base64 PCF/gzip>" }` | `{ "font": { "glyphs": { … }, … } }` |

Errors use an HTTP 4xx/5xx status and `{ "error": "message" }`.

The server enforces the USB.FXD limit of 32 frames. USB.FXD stores a single
global frame delay; the exporter uses the first frame's duration, matching the
format's behaviour.

## Tests

```bash
cargo test
cd frontend && npm run build
```
