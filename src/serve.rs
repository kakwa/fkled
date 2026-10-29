use anyhow::Result;
use base64::engine::general_purpose::STANDARD as BASE64;
use base64::Engine;
use ledfx_editor::export;
use ledfx_editor::font::{self, Font};
use ledfx_editor::model::Animation;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use std::fs;
use std::io::{Cursor, Read};
use std::path::{Component, Path, PathBuf};
use tiny_http::{Header, Request, Response, Server, StatusCode};

const MAX_BODY_BYTES: u64 = 64 * 1024 * 1024;

type ApiResult = std::result::Result<Response<Cursor<Vec<u8>>>, (u16, String)>;

fn main() {
    let mut args = std::env::args().skip(1);
    let port = args
        .next()
        .and_then(|value| value.parse::<u16>().ok())
        .unwrap_or(8080);
    let web_dir = args
        .next()
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("frontend/dist"));

    let address = format!("0.0.0.0:{port}");
    let server = Server::http(&address).unwrap_or_else(|error| {
        eprintln!("failed to bind {address}: {error}");
        std::process::exit(1);
    });

    println!("LEDFX server listening on http://{address}");
    println!("Serving front-end from {}", web_dir.display());
    if !web_dir.join("index.html").exists() {
        println!(
            "warning: {} not found; run `cd frontend && npm install && npm run build`",
            web_dir.join("index.html").display()
        );
    }

    for request in server.incoming_requests() {
        if let Err(error) = handle(request, &web_dir) {
            eprintln!("request failed: {error:#}");
        }
    }
}

fn handle(request: Request, web_dir: &Path) -> Result<()> {
    let method = request.method().as_str().to_string();
    let path = request.url().split('?').next().unwrap_or("/").to_string();

    if method == "OPTIONS" {
        let response = Response::from_data(Vec::new())
            .with_status_code(StatusCode(204))
            .with_header(header("Access-Control-Allow-Origin", "*"))
            .with_header(header("Access-Control-Allow-Methods", "GET, POST, OPTIONS"))
            .with_header(header("Access-Control-Allow-Headers", "Content-Type"));
        request.respond(response)?;
        return Ok(());
    }

    if path == "/api" || path.starts_with("/api/") {
        let mut request = request;
        let response = match route_api(&mut request, &method, &path) {
            Ok(response) => response,
            Err((status, message)) => json_error(status, &message),
        };
        request.respond(response)?;
    } else {
        respond_static(request, web_dir, &path)?;
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// JSON API
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
struct DataPayload {
    data: String,
}

#[derive(Deserialize)]
struct ExportPayload {
    animation: Animation,
}

#[derive(Serialize)]
struct AnimationPayload {
    animation: Animation,
}

#[derive(Serialize)]
struct FilePayload {
    filename: String,
    mime_type: String,
    data: String,
}

#[derive(Serialize)]
struct FontPayload {
    font: Font,
}

#[derive(Serialize)]
struct HealthPayload {
    ok: bool,
    service: &'static str,
    version: &'static str,
}

#[derive(Serialize)]
struct ErrorPayload {
    error: String,
}

fn route_api(request: &mut Request, method: &str, path: &str) -> ApiResult {
    match (method, path) {
        ("GET", "/api/health") => Ok(json_ok(&HealthPayload {
            ok: true,
            service: "ledfx_editor",
            version: env!("CARGO_PKG_VERSION"),
        })),
        ("POST", "/api/import/usb-fxd") => {
            let payload: DataPayload = read_json(request)?;
            let bytes = decode_base64(&payload.data)?;
            let animation =
                export::import_usb_fxd_bytes(&bytes).map_err(|error| (400, error.to_string()))?;
            Ok(json_ok(&AnimationPayload { animation }))
        }
        ("POST", "/api/import/gif") => {
            let payload: DataPayload = read_json(request)?;
            let bytes = decode_base64(&payload.data)?;
            let animation =
                export::import_gif_bytes(&bytes).map_err(|error| (400, error.to_string()))?;
            Ok(json_ok(&AnimationPayload { animation }))
        }
        ("POST", "/api/export/usb-fxd") => {
            let payload: ExportPayload = read_json(request)?;
            let bytes = export::export_usb_fxd_bytes(&payload.animation)
                .map_err(|error| (400, error.to_string()))?;
            Ok(json_ok(&FilePayload {
                filename: safe_filename(&payload.animation.name, "FXD"),
                mime_type: "application/octet-stream".to_string(),
                data: BASE64.encode(bytes),
            }))
        }
        ("POST", "/api/export/gif") => {
            let payload: ExportPayload = read_json(request)?;
            let bytes = export::export_gif_bytes(&payload.animation)
                .map_err(|error| (400, error.to_string()))?;
            Ok(json_ok(&FilePayload {
                filename: safe_filename(&payload.animation.name, "gif"),
                mime_type: "image/gif".to_string(),
                data: BASE64.encode(bytes),
            }))
        }
        ("POST", "/api/fonts/parse") => {
            let payload: DataPayload = read_json(request)?;
            let bytes = decode_base64(&payload.data)?;
            let font = font::parse_font_bytes(&bytes).map_err(|error| (400, error.to_string()))?;
            Ok(json_ok(&FontPayload { font }))
        }
        _ if path == "/api" || path.starts_with("/api/") => {
            Err((404, format!("unknown API endpoint: {method} {path}")))
        }
        _ => Err((404, "not found".to_string())),
    }
}

fn read_json<T: DeserializeOwned>(request: &mut Request) -> std::result::Result<T, (u16, String)> {
    let mut body = Vec::new();
    request
        .as_reader()
        .take(MAX_BODY_BYTES)
        .read_to_end(&mut body)
        .map_err(|error| (400, format!("failed to read request body: {error}")))?;
    serde_json::from_slice(&body).map_err(|error| (400, format!("invalid JSON body: {error}")))
}

fn decode_base64(value: &str) -> std::result::Result<Vec<u8>, (u16, String)> {
    BASE64
        .decode(value.as_bytes())
        .map_err(|error| (400, format!("invalid base64 payload: {error}")))
}

fn json_ok<T: Serialize>(value: &T) -> Response<Cursor<Vec<u8>>> {
    let body = serde_json::to_vec(value).unwrap_or_else(|error| {
        format!(r#"{{"error":"failed to encode response: {error}"}}"#).into_bytes()
    });
    Response::from_data(body)
        .with_status_code(StatusCode(200))
        .with_header(header("Content-Type", "application/json; charset=utf-8"))
        .with_header(header("Cache-Control", "no-store"))
        .with_header(header("Access-Control-Allow-Origin", "*"))
}

fn json_error(status: u16, message: &str) -> Response<Cursor<Vec<u8>>> {
    let body = serde_json::to_vec(&ErrorPayload {
        error: message.to_string(),
    })
    .unwrap_or_else(|_| br#"{"error":"internal error"}"#.to_vec());
    Response::from_data(body)
        .with_status_code(StatusCode(status))
        .with_header(header("Content-Type", "application/json; charset=utf-8"))
        .with_header(header("Cache-Control", "no-store"))
        .with_header(header("Access-Control-Allow-Origin", "*"))
}

fn safe_filename(name: &str, extension: &str) -> String {
    let stem: String = name
        .trim()
        .chars()
        .map(|character| {
            if character.is_alphanumeric() || matches!(character, '-' | '_' | ' ') {
                character
            } else {
                '_'
            }
        })
        .take(80)
        .collect();
    let stem = stem.trim();
    let stem = if stem.is_empty() { "animation" } else { stem };
    format!("{stem}.{extension}")
}

// ---------------------------------------------------------------------------
// Static front-end
// ---------------------------------------------------------------------------

fn respond_static(request: Request, web_dir: &Path, url_path: &str) -> Result<()> {
    let method = request.method().as_str();
    if method != "GET" && method != "HEAD" {
        let response = Response::from_string("method not allowed")
            .with_status_code(StatusCode(405))
            .with_header(header("Content-Type", "text/plain; charset=utf-8"));
        request.respond(response)?;
        return Ok(());
    }

    let relative = match percent_decode(url_path.trim_start_matches('/')) {
        Some(relative) => relative,
        None => return respond_text(request, 400, "invalid path encoding"),
    };
    let relative_path = Path::new(&relative);
    let has_extension = relative_path.extension().is_some();

    let mut file_path = web_dir.to_path_buf();
    if relative.is_empty() {
        file_path.push("index.html");
    } else {
        if relative_path
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
        {
            return respond_text(request, 400, "invalid path");
        }
        file_path.push(relative_path);
    }

    if file_path.is_dir() {
        file_path.push("index.html");
    }

    let mut data = match fs::read(&file_path) {
        Ok(data) => Some(data),
        Err(_) => None,
    };

    // SPA fallback: unknown extension-less routes render the app.
    if data.is_none() && !has_extension {
        data = fs::read(web_dir.join("index.html")).ok();
        if data.is_some() {
            file_path = web_dir.join("index.html");
        }
    }

    let Some(data) = data else {
        if !web_dir.join("index.html").exists() {
            return respond_text(
                request,
                503,
                "Front-end build not found. Run `cd frontend && npm install && npm run build`.",
            );
        }
        return respond_text(request, 404, "not found");
    };

    let mime = mime_guess::from_path(&file_path)
        .first_or_octet_stream()
        .essence_str()
        .to_string();
    let cache = if relative.starts_with("assets/") {
        "public, max-age=31536000, immutable"
    } else {
        "no-cache"
    };

    let response = Response::from_data(data)
        .with_status_code(StatusCode(200))
        .with_header(header("Content-Type", &mime))
        .with_header(header("Cache-Control", cache))
        .with_header(header("Access-Control-Allow-Origin", "*"));
    request.respond(response)?;
    Ok(())
}

fn respond_text(request: Request, status: u16, message: &str) -> Result<()> {
    let response = Response::from_string(message)
        .with_status_code(StatusCode(status))
        .with_header(header("Content-Type", "text/plain; charset=utf-8"))
        .with_header(header("Cache-Control", "no-store"));
    request.respond(response)?;
    Ok(())
}

fn percent_decode(value: &str) -> Option<String> {
    let bytes = value.as_bytes();
    let mut output = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' {
            let high = hex_value(*bytes.get(index + 1)?)?;
            let low = hex_value(*bytes.get(index + 2)?)?;
            output.push((high << 4) | low);
            index += 3;
        } else {
            output.push(bytes[index]);
            index += 1;
        }
    }
    String::from_utf8(output).ok()
}

fn hex_value(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

fn header(name: &str, value: &str) -> Header {
    Header::from_bytes(name.as_bytes(), value.as_bytes())
        .unwrap_or_else(|_| panic!("invalid header {name}: {value}"))
}
