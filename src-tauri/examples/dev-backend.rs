//! A development backend for the frontend in a browser: `cargo run -p agentlens-app --example dev-backend`.
//!
//! It answers the same commands as the Tauri application with the data of this
//! machine, on 127.0.0.1 only, and never writes anything. Open the frontend
//! (`pnpm start`) with `?backend=http://127.0.0.1:1421` to use it.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};

use agentlens_app_lib::api::{self, Context, Filters};
use serde::Serialize;
use serde_json::Value;

fn respond(stream: &mut TcpStream, code: u16, body: &str) {
    let _ = write!(
        stream,
        "HTTP/1.1 {code} X\r\ncontent-type: application/json\r\naccess-control-allow-origin: *\r\naccess-control-allow-headers: content-type\r\naccess-control-allow-methods: POST, OPTIONS\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
        body.len()
    );
}

fn json<T: Serialize>(r: Result<api::Answer<T>, String>) -> Result<String, String> {
    r.and_then(|a| serde_json::to_string(&a).map_err(|e| e.to_string()))
}

fn handle(ctx: &Context, command: &str, args: &Value) -> Result<String, String> {
    let filters: Filters =
        serde_json::from_value(args.get("filters").cloned().unwrap_or(Value::Null))
            .unwrap_or_default();
    let text = |key: &str| {
        args.get(key)
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned()
    };
    match command {
        "refresh" => json(api::refresh(ctx)),
        "overview" => json(api::overview(ctx, &filters)),
        "skills" => json(api::skills(ctx, &filters)),
        "context" => json(api::context(ctx, &filters)),
        "unused" => json(api::unused(ctx, &filters)),
        "flow" => json(api::flow(ctx, &filters)),
        "cost" => {
            let what_if = args
                .get("whatIf")
                .and_then(Value::as_str)
                .map(str::to_owned);
            json(api::cost(ctx, &filters, &text("by"), what_if.as_deref()))
        }
        "efficiency" => json(api::efficiency(ctx, &filters, &text("by"))),
        "compare" => json(api::compare(ctx, &filters)),
        "sessions" => json(api::sessions(ctx, &filters)),
        "timeline" => json(api::timeline(ctx, &filters, &text("source"), &text("id"))),
        other => Err(format!("unknown command {other}")),
    }
}

fn serve(ctx: &Context, mut stream: TcpStream) {
    let mut reader = BufReader::new(stream.try_clone().expect("clone stream"));
    let mut request_line = String::new();
    if reader.read_line(&mut request_line).unwrap_or(0) == 0 {
        return;
    }
    let mut length = 0usize;
    loop {
        let mut line = String::new();
        if reader.read_line(&mut line).unwrap_or(0) == 0 || line == "\r\n" {
            break;
        }
        if let Some(v) = line.to_lowercase().strip_prefix("content-length:") {
            length = v.trim().parse().unwrap_or(0);
        }
    }
    if request_line.starts_with("OPTIONS") {
        return respond(&mut stream, 204, "");
    }
    let mut body = vec![0u8; length];
    let _ = reader.read_exact(&mut body);
    let command = request_line
        .split_whitespace()
        .nth(1)
        .unwrap_or("/")
        .trim_start_matches('/')
        .to_owned();
    let args: Value = serde_json::from_slice(&body).unwrap_or(Value::Null);
    match handle(ctx, &command, &args) {
        Ok(body) => respond(&mut stream, 200, &body),
        Err(e) => respond(&mut stream, 500, &Value::String(e).to_string()),
    }
}

fn main() {
    let ctx = Context::system().expect("cannot determine the home directory");
    let listener = TcpListener::bind("127.0.0.1:1421").expect("port 1421 is taken");
    eprintln!("dev backend on http://127.0.0.1:1421 (read-only, this machine's data)");
    for stream in listener.incoming().flatten() {
        serve(&ctx, stream);
    }
}
