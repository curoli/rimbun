use std::{
    env, fs,
    io::{BufRead, BufReader, Write},
    net::{TcpListener, TcpStream},
    path::{Component, Path, PathBuf},
    thread,
};

use anyhow::{Context, Result, bail};

fn main() -> Result<()> {
    let mut args = env::args().skip(1);
    let root = args
        .next()
        .map(PathBuf::from)
        .context("usage: rimbun-static-server <ROOT> <PORT>")?
        .canonicalize()
        .context("failed to resolve static root")?;
    let port = args
        .next()
        .context("usage: rimbun-static-server <ROOT> <PORT>")?
        .parse::<u16>()?;
    if args.next().is_some() {
        bail!("usage: rimbun-static-server <ROOT> <PORT>");
    }

    let listener = TcpListener::bind(("127.0.0.1", port))?;
    println!("Serving {} at http://127.0.0.1:{port}/", root.display());
    for stream in listener.incoming() {
        match stream {
            Ok(stream) => {
                let root = root.clone();
                thread::spawn(move || {
                    let _ = handle_request(stream, &root);
                });
            }
            Err(error) => eprintln!("failed to accept connection: {error}"),
        }
    }
    Ok(())
}

fn handle_request(mut stream: TcpStream, root: &Path) -> Result<()> {
    let mut reader = BufReader::new(stream.try_clone()?);
    let mut request_line = String::new();
    reader.read_line(&mut request_line)?;
    let mut parts = request_line.split_whitespace();
    let method = parts.next().unwrap_or_default();
    let target = parts.next().unwrap_or_default();
    if !matches!(method, "GET" | "HEAD") {
        return send_response(
            &mut stream,
            method,
            405,
            "text/plain",
            b"method not allowed\n",
        );
    }

    let requested = safe_request_path(target)?;
    let mut file = root.join(requested);
    if file.is_dir() {
        file = file.join("index.html");
    }
    if !file.is_file() {
        file = root.join("index.html");
    }
    if !file.is_file() {
        return send_response(&mut stream, method, 404, "text/plain", b"not found\n");
    }

    let body = fs::read(&file)?;
    send_response(&mut stream, method, 200, content_type(&file), &body)
}

fn safe_request_path(target: &str) -> Result<PathBuf> {
    let raw_path = target.split('?').next().unwrap_or_default();
    let decoded = percent_decode(raw_path)?;
    let mut safe = PathBuf::new();
    for component in Path::new(decoded.trim_start_matches('/')).components() {
        match component {
            Component::Normal(part) => safe.push(part),
            Component::CurDir => {}
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                bail!("invalid request path")
            }
        }
    }
    Ok(safe)
}

fn percent_decode(value: &str) -> Result<String> {
    let bytes = value.as_bytes();
    let mut output = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' {
            if index + 2 >= bytes.len() {
                bail!("invalid percent encoding");
            }
            let high = hex_value(bytes[index + 1])?;
            let low = hex_value(bytes[index + 2])?;
            output.push(high * 16 + low);
            index += 3;
        } else {
            output.push(bytes[index]);
            index += 1;
        }
    }
    String::from_utf8(output).context("request path is not UTF-8")
}

fn hex_value(value: u8) -> Result<u8> {
    match value {
        b'0'..=b'9' => Ok(value - b'0'),
        b'a'..=b'f' => Ok(value - b'a' + 10),
        b'A'..=b'F' => Ok(value - b'A' + 10),
        _ => bail!("invalid percent encoding"),
    }
}

fn content_type(path: &Path) -> &'static str {
    match path.extension().and_then(|value| value.to_str()) {
        Some("css") => "text/css; charset=utf-8",
        Some("html") => "text/html; charset=utf-8",
        Some("js") | Some("mjs") => "text/javascript; charset=utf-8",
        Some("json") => "application/json; charset=utf-8",
        Some("svg") => "image/svg+xml",
        Some("png") => "image/png",
        Some("jpg") | Some("jpeg") => "image/jpeg",
        Some("webp") => "image/webp",
        Some("woff") => "font/woff",
        Some("woff2") => "font/woff2",
        _ => "application/octet-stream",
    }
}

fn send_response(
    stream: &mut TcpStream,
    method: &str,
    status: u16,
    content_type: &str,
    body: &[u8],
) -> Result<()> {
    let reason = match status {
        200 => "OK",
        404 => "Not Found",
        405 => "Method Not Allowed",
        _ => "Error",
    };
    write!(
        stream,
        "HTTP/1.1 {status} {reason}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    )?;
    if method != "HEAD" {
        stream.write_all(body)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_paths_reject_parent_segments_after_decoding() {
        assert!(safe_request_path("/%2e%2e/secret").is_err());
        assert_eq!(
            safe_request_path("/documents/example?lang=en").expect("safe request path"),
            PathBuf::from("documents/example")
        );
    }

    #[test]
    fn content_types_cover_built_frontend_assets() {
        assert_eq!(
            content_type(Path::new("index.html")),
            "text/html; charset=utf-8"
        );
        assert_eq!(
            content_type(Path::new("app.js")),
            "text/javascript; charset=utf-8"
        );
        assert_eq!(
            content_type(Path::new("app.css")),
            "text/css; charset=utf-8"
        );
    }
}
