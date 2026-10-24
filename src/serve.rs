use std::fs;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let port = if args.len() > 1 { args[1].parse().unwrap_or(8080) } else { 8080 };
    let dir = PathBuf::from("dist");
    
    let listener = TcpListener::bind(format!("0.0.0.0:{}", port)).unwrap();
    println!("Serving {} at http://0.0.0.0:{}", dir.display(), port);
    
    for stream in listener.incoming() {
        if let Ok(mut stream) = stream {
            let dir_clone = dir.clone();
            std::thread::spawn(move || {
                handle_request(&mut stream, &dir_clone);
            });
        }
    }
}

fn handle_request(stream: &mut TcpStream, dir: &PathBuf) {
    let mut buffer = [0; 4096];
    if let Ok(size) = stream.read(&mut buffer) {
        let request = String::from_utf8_lossy(&buffer[..size]);
        let mut parts = request.split_whitespace();
        let _method = parts.next();
        let path = parts.next().unwrap_or("/");
        
        let path = if path == "/" { "/index.html" } else { path };
        let file_path = dir.join(&path[1..]);
        
        let (status, content_type, content) = match fs::read(&file_path) {
            Ok(data) => {
                let ct = match file_path.extension().and_then(|e| e.to_str()) {
                    Some("html") => "text/html",
                    Some("js") => "application/javascript",
                    Some("wasm") => "application/wasm",
                    Some("json") => "application/json",
                    Some("css") => "text/css",
                    Some("png") => "image/png",
                    Some("gif") => "image/gif",
                    _ => "application/octet-stream",
                };
                ("200 OK", ct, data)
            }
            Err(_) => ("404 NOT FOUND", "text/html", b"<h1>404</h1>".to_vec()),
        };
        
        let headers = format!(
            "HTTP/1.1 {}\r\nContent-Type: {}\r\nContent-Length: {}\r\nCross-Origin-Embedder-Policy: require-corp\r\nCross-Origin-Opener-Policy: same-origin\r\nCache-Control: no-cache\r\nConnection: close\r\n\r\n",
            status,
            content_type,
            content.len()
        );
        
        let _ = stream.write_all(headers.as_bytes());
        let _ = stream.write_all(&content);
    }
}
