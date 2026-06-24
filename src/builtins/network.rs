use std::io::{Read, Write};
use std::net::TcpStream;
use std::rc::Rc;
use std::time::Duration;

use crate::env::{Env, env_set};
use crate::expr::Expr;
use crate::gc::Heap;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Pull a `&str` out of an `Expr::Str`, or return a typed error.
fn str_arg<'a>(expr: &'a Expr, fn_name: &str) -> Result<&'a str, String> {
    match expr {
        Expr::Str(s) => Ok(s.as_str()),
        other => Err(format!("{}: expected a string, got {:?}", fn_name, other)),
    }
}

/// Pull an `f64` that represents a port number (1-65535).
fn port_arg(expr: &Expr, fn_name: &str) -> Result<u16, String> {
    match expr {
        Expr::Int(n) => {
            let p = *n as u64;
            if p == 0 || p > 65535 {
                Err(format!("{}: port must be 1-65535, got {}", fn_name, p))
            } else {
                Ok(p as u16)
            }
        }
        Expr::Float(n) => {
            let p = *n as u64;
            if p == 0 || p > 65535 {
                Err(format!("{}: port must be 1-65535, got {}", fn_name, p))
            } else {
                Ok(p as u16)
            }
        }
        other => Err(format!(
            "{}: expected a port number, got {:?}",
            fn_name, other
        )),
    }
}

/// Build a minimal HTTP/1.1 request string.
fn build_http_request(method: &str, host: &str, path: &str, body: Option<&str>) -> String {
    let body_bytes = body.unwrap_or("");
    let mut req = format!(
        "{method} {path} HTTP/1.1\r\n\
         Host: {host}\r\n\
         Connection: close\r\n\
         User-Agent: lisp-net/1.0\r\n"
    );
    if let Some(b) = body {
        req.push_str(&format!(
            "Content-Type: application/x-www-form-urlencoded\r\n\
             Content-Length: {}\r\n",
            b.len()
        ));
    } else {
        req.push_str("Content-Length: 0\r\n");
    }
    req.push_str("\r\n");
    req.push_str(body_bytes);
    req
}

/// Split an HTTP response into (status_line, headers, body).
fn parse_http_response(raw: &str) -> (String, String, String) {
    let mut parts = raw.splitn(2, "\r\n\r\n");
    let head = parts.next().unwrap_or("").to_string();
    let body = parts.next().unwrap_or("").to_string();
    let mut head_lines = head.splitn(2, "\r\n");
    let status = head_lines.next().unwrap_or("").to_string();
    let headers = head_lines.next().unwrap_or("").to_string();
    (status, headers, body)
}

/// Execute a blocking TCP+HTTP exchange, return the response as an
/// `Expr::List([status, headers, body])` where every element is `Expr::Str`.
fn http_request(
    method: &str,
    host: &str,
    port: u16,
    path: &str,
    body: Option<&str>,
) -> Result<Expr, String> {
    let addr = format!("{}:{}", host, port);
    let mut stream = TcpStream::connect(&addr).map_err(|e| {
        format!(
            "http-{}: connect to {} failed: {}",
            method.to_lowercase(),
            addr,
            e
        )
    })?;

    stream
        .set_read_timeout(Some(Duration::from_secs(10)))
        .map_err(|e| format!("http-{}: set timeout failed: {}", method.to_lowercase(), e))?;

    let request = build_http_request(method, host, path, body);
    stream
        .write_all(request.as_bytes())
        .map_err(|e| format!("http-{}: write failed: {}", method.to_lowercase(), e))?;

    let mut response = String::new();
    stream
        .read_to_string(&mut response)
        .map_err(|e| format!("http-{}: read failed: {}", method.to_lowercase(), e))?;

    let (status, headers, body_out) = parse_http_response(&response);
    Ok(Expr::List(vec![
        Expr::Str(status),
        Expr::Str(headers),
        Expr::Str(body_out),
    ]))
}

// ---------------------------------------------------------------------------
// Registration
// ---------------------------------------------------------------------------

pub fn register_network(env: Env, heap: &mut Heap) {
    register_tcp(env, heap);
    register_http(env, heap);
}

// ---------------------------------------------------------------------------
// TCP
// ---------------------------------------------------------------------------

fn register_tcp(env: Env, heap: &mut Heap) {
    // (tcp-connect host port message) -> response-string
    //
    // Opens a TCP connection to `host:port`, sends `message`, reads the full
    // reply (until the peer closes the connection), and returns it as a string.
    //
    // Example: (tcp-connect "echo.example.com" 7 "hello")
    env_set(
        heap,
        env,
        "tcp-connect".into(),
        Expr::Func(Rc::new(|args, _heap| {
            if args.len() != 3 {
                return Err("tcp-connect: expects exactly 3 arguments (host port message)".into());
            }
            let host = str_arg(&args[0], "tcp-connect")?;
            let port = port_arg(&args[1], "tcp-connect")?;
            let message = str_arg(&args[2], "tcp-connect")?;

            let addr = format!("{}:{}", host, port);
            let mut stream = TcpStream::connect(&addr)
                .map_err(|e| format!("tcp-connect: could not connect to {}: {}", addr, e))?;

            stream
                .set_read_timeout(Some(Duration::from_secs(10)))
                .map_err(|e| format!("tcp-connect: set timeout failed: {}", e))?;

            stream
                .write_all(message.as_bytes())
                .map_err(|e| format!("tcp-connect: write failed: {}", e))?;

            let mut response = String::new();
            stream
                .read_to_string(&mut response)
                .map_err(|e| format!("tcp-connect: read failed: {}", e))?;

            Ok(Expr::Str(response))
        })),
    );

    // (tcp-send host port message) -> bytes-sent (number)
    //
    // Fire-and-forget variant: connects, sends the message, and returns the
    // number of bytes written without waiting for a reply.
    //
    // Example: (tcp-send "logger.internal" 514 "hello syslog")
    env_set(
        heap,
        env,
        "tcp-send".into(),
        Expr::Func(Rc::new(|args, _heap| {
            if args.len() != 3 {
                return Err("tcp-send: expects exactly 3 arguments (host port message)".into());
            }
            let host = str_arg(&args[0], "tcp-send")?;
            let port = port_arg(&args[1], "tcp-send")?;
            let message = str_arg(&args[2], "tcp-send")?;

            let addr = format!("{}:{}", host, port);
            let mut stream = TcpStream::connect(&addr)
                .map_err(|e| format!("tcp-send: could not connect to {}: {}", addr, e))?;

            let n = stream
                .write(message.as_bytes())
                .map_err(|e| format!("tcp-send: write failed: {}", e))?;

            Ok(Expr::Int(n as i64))
        })),
    );
}

// ---------------------------------------------------------------------------
// HTTP (plain HTTP/1.1 over TCP, no TLS)
// ---------------------------------------------------------------------------

fn register_http(env: Env, heap: &mut Heap) {
    // (http-get host port path) -> (list status headers body)
    //
    // Performs a blocking HTTP GET and returns a 3-element list:
    //   0 - status line  e.g. "HTTP/1.1 200 OK"
    //   1 - raw headers block
    //   2 - response body
    //
    // Example: (http-get "example.com" 80 "/index.html")
    env_set(
        heap,
        env,
        "http-get".into(),
        Expr::Func(Rc::new(|args, _heap| {
            if args.len() != 3 {
                return Err("http-get: expects exactly 3 arguments (host port path)".into());
            }
            let host = str_arg(&args[0], "http-get")?;
            let port = port_arg(&args[1], "http-get")?;
            let path = str_arg(&args[2], "http-get")?;
            http_request("GET", host, port, path, None)
        })),
    );

    // (http-post host port path body) -> (list status headers body)
    //
    // Performs a blocking HTTP POST with the given body string.
    // Content-Type is set to application/x-www-form-urlencoded.
    //
    // Example: (http-post "api.example.com" 80 "/data" "key=value&foo=bar")
    env_set(
        heap,
        env,
        "http-post".into(),
        Expr::Func(Rc::new(|args, _heap| {
            if args.len() != 4 {
                return Err("http-post: expects exactly 4 arguments (host port path body)".into());
            }
            let host = str_arg(&args[0], "http-post")?;
            let port = port_arg(&args[1], "http-post")?;
            let path = str_arg(&args[2], "http-post")?;
            let body = str_arg(&args[3], "http-post")?;
            http_request("POST", host, port, path, Some(body))
        })),
    );

    // (http-status response) -> status-code (number)
    //
    // Extracts the numeric status code from the status line returned by
    // http-get / http-post.
    //
    // Example: (http-status (http-get "example.com" 80 "/"))  => 200
    env_set(
        heap,
        env,
        "http-status".into(),
        Expr::Func(Rc::new(|args, _heap| {
            if args.len() != 1 {
                return Err("http-status: expects exactly 1 argument (response)".into());
            }
            let response = match &args[0] {
                Expr::List(l) if l.len() == 3 => l,
                other => {
                    return Err(format!(
                        "http-status: expected a 3-element response list, got {:?}",
                        other
                    ));
                }
            };
            let status_line = str_arg(&response[0], "http-status")?;
            // e.g. "HTTP/1.1 200 OK" -> "200"
            let code_str = status_line
                .split_whitespace()
                .nth(1)
                .ok_or_else(|| format!("http-status: malformed status line: {:?}", status_line))?;
            let code: f64 = code_str
                .parse()
                .map_err(|_| format!("http-status: non-numeric status code: {:?}", code_str))?;
            Ok(Expr::Int(code as i64))
        })),
    );

    // (http-body response) -> body-string
    //
    // Extracts the body string from a response list.
    //
    // Example: (http-body (http-get "example.com" 80 "/"))
    env_set(
        heap,
        env,
        "http-body".into(),
        Expr::Func(Rc::new(|args, _heap| {
            if args.len() != 1 {
                return Err("http-body: expects exactly 1 argument (response)".into());
            }
            let response = match &args[0] {
                Expr::List(l) if l.len() == 3 => l,
                other => {
                    return Err(format!(
                        "http-body: expected a 3-element response list, got {:?}",
                        other
                    ));
                }
            };
            Ok(response[2].clone())
        })),
    );
}
