use std::io::{Read, Write};
use std::rc::Rc;

use crate::{
    builtins::str_arg,
    env::{Env, env_set},
    expr::Expr,
    gc::Heap,
};

pub fn register_terminal(env: Env, heap: &mut Heap) {
    // (write str) — write string to stdout without newline, return ()
    env_set(
        heap,
        env,
        "write".into(),
        Expr::Func(Rc::new(|args, _heap| {
            if args.len() != 1 {
                return Err("write: expects exactly 1 argument (a string)".into());
            }
            let s = str_arg(&args[0])?;
            std::io::stdout()
                .write_all(s.as_bytes())
                .and_then(|_| std::io::stdout().flush())
                .map_err(|e| format!("write: {}", e))?;
            Ok(Expr::List(vec![]))
        })),
    );

    // (writeline str) — write string + newline, return ()
    env_set(
        heap,
        env,
        "writeline".into(),
        Expr::Func(Rc::new(|args, _heap| {
            if args.len() != 1 {
                return Err("writeline: expects exactly 1 argument (a string)".into());
            }
            let s = str_arg(&args[0])?;
            std::io::stdout()
                .write_all(s.as_bytes())
                .and_then(|_| std::io::stdout().write_all(b"\n"))
                .and_then(|_| std::io::stdout().flush())
                .map_err(|e| format!("writeline: {}", e))?;
            Ok(Expr::List(vec![]))
        })),
    );

    // (read-byte) — read one byte from stdin, return int 0-255, or -1 on EOF
    env_set(
        heap,
        env,
        "read-byte".into(),
        Expr::Func(Rc::new(|args, _heap| {
            if !args.is_empty() {
                return Err("read-byte: expects 0 arguments".into());
            }
            let mut buf = [0u8; 1];
            match std::io::stdin().read_exact(&mut buf) {
                Ok(()) => Ok(Expr::Int(buf[0] as i64)),
                Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => {
                    Ok(Expr::Int(-1))
                }
                Err(e) => Err(format!("read-byte: {}", e)),
            }
        })),
    );

    // (raw-mode bool) — enable/disable raw terminal mode
    // On non-Unix platforms this is a no-op (always succeeds).
    env_set(
        heap,
        env,
        "raw-mode".into(),
        Expr::Func(Rc::new(|args, _heap| {
            if args.len() != 1 {
                return Err("raw-mode: expects 1 argument (#t or #f)".into());
            }
            let enable = matches!(&args[0], Expr::Bool(true));
            set_raw_mode(enable).map_err(|e| format!("raw-mode: {}", e))?;
            Ok(Expr::List(vec![]))
        })),
    );

    // (terminal-size) → list (rows cols)
    env_set(
        heap,
        env,
        "terminal-size".into(),
        Expr::Func(Rc::new(|args, _heap| {
            if !args.is_empty() {
                return Err("terminal-size: expects 0 arguments".into());
            }
            let (rows, cols) = terminal_size_impl().unwrap_or((24, 80));
            Ok(Expr::List(vec![Expr::Int(rows), Expr::Int(cols)]))
        })),
    );

    // (exit [code]) — exit the process
    env_set(
        heap,
        env,
        "exit".into(),
        Expr::Func(Rc::new(|args, _heap| {
            let code = if args.is_empty() {
                0
            } else if args.len() == 1 {
                match &args[0] {
                    Expr::Int(n) => *n as i32,
                    Expr::Float(n) => *n as i32,
                    _ => return Err("exit: expected a number or no arguments".into()),
                }
            } else {
                return Err("exit: expects 0 or 1 arguments".into());
            };
            std::process::exit(code);
        })),
    );
}

pub fn register_string_extras(env: Env, heap: &mut Heap) {
    // (string-ref s index) → int (Unicode codepoint), or -1 if out of range
    env_set(
        heap,
        env,
        "string-ref".into(),
        Expr::Func(Rc::new(|args, _heap| {
            if args.len() != 2 {
                return Err("string-ref: expects (string-ref s index)".into());
            }
            let s = str_arg(&args[0])?;
            let idx = match &args[1] {
                Expr::Int(n) => *n,
                _ => return Err("string-ref: index must be an integer".into()),
            };
            let ch = s.chars().nth(idx as usize);
            match ch {
                Some(c) => Ok(Expr::Int(c as i64)),
                None => Ok(Expr::Int(-1)),
            }
        })),
    );

    // (string-split s separator) → list of strings
    env_set(
        heap,
        env,
        "string-split".into(),
        Expr::Func(Rc::new(|args, _heap| {
            if args.len() != 2 {
                return Err("string-split: expects (string-split s separator)".into());
            }
            let s = str_arg(&args[0])?;
            let sep = str_arg(&args[1])?;
            let parts: Vec<Expr> = if sep.is_empty() {
                s.chars().map(|c| Expr::Str(c.to_string())).collect()
            } else {
                s.split(sep).map(|p| Expr::Str(p.to_string())).collect()
            };
            Ok(Expr::List(parts))
        })),
    );

    // (string-index-of s substr) → int index or -1 if not found
    env_set(
        heap,
        env,
        "string-index-of".into(),
        Expr::Func(Rc::new(|args, _heap| {
            if args.len() != 2 {
                return Err("string-index-of: expects (string-index-of s substr)".into());
            }
            let s = str_arg(&args[0])?;
            let sub = str_arg(&args[1])?;
            match s.find(sub) {
                Some(pos) => Ok(Expr::Int(pos as i64)),
                None => Ok(Expr::Int(-1)),
            }
        })),
    );

    // (string-contains? s substr) → bool
    env_set(
        heap,
        env,
        "string-contains?".into(),
        Expr::Func(Rc::new(|args, _heap| {
            if args.len() != 2 {
                return Err("string-contains?: expects (string-contains? s substr)".into());
            }
            let s = str_arg(&args[0])?;
            let sub = str_arg(&args[1])?;
            Ok(Expr::Bool(s.contains(sub)))
        })),
    );
}

// ── Platform-specific helpers ────────────────────────────────────────────

#[cfg(unix)]
fn set_raw_mode(enable: bool) -> Result<(), String> {
    use std::mem::MaybeUninit;
    let fd = 0; // stdin

    // Get current terminal attributes
    let mut termios = MaybeUninit::<libc::termios>::uninit();
    let ret = unsafe { libc::tcgetattr(fd, termios.as_mut_ptr()) };
    if ret != 0 {
        return Err(format!("tcgetattr failed: {}", std::io::Error::last_os_error()));
    }
    let mut t = unsafe { termios.assume_init() };

    if enable {
        // Raw mode: disable canonical mode, echo, signals, etc.
        t.c_lflag &= !(libc::ICANON | libc::ECHO | libc::ISIG | libc::IEXTEN);
        t.c_iflag &= !(libc::IXON | libc::ICRNL | libc::BRKINT | libc::INPCK | libc::ISTRIP);
        t.c_oflag &= !(libc::OPOST);
        t.c_cflag |= libc::CS8;
        // VMIN = 1, VTIME = 0: read returns as soon as 1 byte is available
        t.c_cc[libc::VMIN as usize] = 1;
        t.c_cc[libc::VTIME as usize] = 0;
    } else {
        // Cooked mode: sensible defaults (echo, canonical, signals)
        t.c_lflag |= libc::ICANON | libc::ECHO | libc::ISIG;
        t.c_iflag |= libc::IXON | libc::ICRNL;
        t.c_oflag |= libc::OPOST;
    }

    let ret = unsafe { libc::tcsetattr(fd, libc::TCSAFLUSH, &t) };
    if ret != 0 {
        return Err(format!("tcsetattr failed: {}", std::io::Error::last_os_error()));
    }
    Ok(())
}

#[cfg(not(unix))]
fn set_raw_mode(_enable: bool) -> Result<(), String> {
    Ok(())
}

#[cfg(unix)]
fn terminal_size_impl() -> Result<(i64, i64), String> {
    use std::mem::MaybeUninit;
    let mut ws = MaybeUninit::<libc::winsize>::uninit();
    let ret = unsafe { libc::ioctl(libc::STDOUT_FILENO, libc::TIOCGWINSZ, ws.as_mut_ptr()) };
    if ret != 0 {
        return Err(format!(
            "ioctl(TIOCGWINSZ) failed: {}",
            std::io::Error::last_os_error()
        ));
    }
    let ws = unsafe { ws.assume_init() };
    Ok((ws.ws_row as i64, ws.ws_col as i64))
}

#[cfg(not(unix))]
fn terminal_size_impl() -> Result<(i64, i64), String> {
    Ok((24, 80))
}
