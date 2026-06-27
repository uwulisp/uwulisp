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

/// Pull a port number (1-65535) from an `Expr::Int`.
fn port_arg(expr: &Expr, fn_name: &str) -> Result<u16, String> {
    match expr {
        Expr::Int(n) => {
            let p = *n;
            if p < 1 || p > 65535 {
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
    }
    req.push_str("\r\n");
    if let Some(b) = body {
        req.push_str(b);
    }
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

/// Connect to `host:port`, optionally setting a read timeout.
fn tcp_connect_stream(
    host: &str,
    port: u16,
    timeout: Option<Duration>,
    fn_name: &str,
) -> Result<TcpStream, String> {
    let addr = format!("{}:{}", host, port);
    let stream = TcpStream::connect(&addr)
        .map_err(|e| format!("{}: connect to {} failed: {}", fn_name, addr, e))?;
    if let Some(d) = timeout {
        stream
            .set_read_timeout(Some(d))
            .map_err(|e| format!("{}: set timeout failed: {}", fn_name, e))?;
    }
    Ok(stream)
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
    let fn_name = format!("http-{}", method.to_lowercase());
    let mut stream = tcp_connect_stream(host, port, Some(Duration::from_secs(10)), &fn_name)?;

    let request = build_http_request(method, host, path, body);
    stream
        .write_all(request.as_bytes())
        .map_err(|e| format!("{}: write failed: {}", fn_name, e))?;

    let mut response = String::new();
    stream
        .read_to_string(&mut response)
        .map_err(|e| format!("{}: read failed: {}", fn_name, e))?;

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

            let mut stream = tcp_connect_stream(host, port, Some(Duration::from_secs(10)), "tcp-connect")?;

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

            let mut stream = tcp_connect_stream(host, port, None, "tcp-send")?;

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
    // Register an HTTP method function (GET, POST, PUT, PATCH, DELETE).
    fn register_method(
        env: Env,
        heap: &mut Heap,
        name: &'static str,
        method: &'static str,
        has_body: bool,
    ) {
        let expected = if has_body { 4 } else { 3 };
        env_set(
            heap,
            env,
            name.into(),
            Expr::Func(Rc::new(move |args, _heap| {
                if args.len() != expected {
                    return Err(format!(
                        "{}: expects exactly {} arguments (host port path{})",
                        name,
                        expected,
                        if has_body { " body)" } else { ")" }
                    ));
                }
                let host = str_arg(&args[0], name)?;
                let port = port_arg(&args[1], name)?;
                let path = str_arg(&args[2], name)?;
                let body = if has_body {
                    Some(str_arg(&args[3], name)?)
                } else {
                    None
                };
                http_request(method, host, port, path, body)
            })),
        );
    }

    register_method(env, heap, "http-get", "GET", false);
    register_method(env, heap, "http-post", "POST", true);
    register_method(env, heap, "http-put", "PUT", true);
    register_method(env, heap, "http-patch", "PATCH", true);
    register_method(env, heap, "http-delete", "DELETE", false);

    // (http-status response) -> status-code (number)
    //
    // Extracts the numeric status code from the status line.
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
            let code_str = status_line
                .split_whitespace()
                .nth(1)
                .ok_or_else(|| format!("http-status: malformed status line: {:?}", status_line))?;
            let code: i64 = code_str
                .parse()
                .map_err(|_| format!("http-status: non-numeric status code: {:?}", code_str))?;
            Ok(Expr::Int(code))
        })),
    );

    // (http-body response) -> body-string
    env_set(
        heap,
        env,
        "http-body".into(),
        Expr::Func(Rc::new(response_accessor(2, "http-body"))),
    );

    // (http-headers response) -> headers-string
    env_set(
        heap,
        env,
        "http-headers".into(),
        Expr::Func(Rc::new(response_accessor(1, "http-headers"))),
    );
}

fn response_accessor(index: usize, name: &'static str) -> impl Fn(&[Expr], &mut Heap) -> Result<Expr, String> {
    move |args, _heap| {
        if args.len() != 1 {
            return Err(format!("{}: expects exactly 1 argument (response)", name));
        }
        let response = match &args[0] {
            Expr::List(l) if l.len() == 3 => l,
            other => {
                return Err(format!(
                    "{}: expected a 3-element response list, got {:?}",
                    name, other
                ));
            }
        };
        Ok(response[index].clone())
    }
}
