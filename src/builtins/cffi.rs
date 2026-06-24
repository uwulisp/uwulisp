use std::ffi::CString;
use std::rc::Rc;

use crate::env::{Env, env_set};
use crate::expr::Expr;
use crate::gc::Heap;

#[cfg(target_family = "unix")]
unsafe extern "C" {
    fn dlopen(filename: *const i8, flag: i32) -> *mut u8;
    fn dlsym(handle: *mut u8, symbol: *const i8) -> *mut u8;
    fn dlclose(handle: *mut u8) -> i32;
}

#[cfg(target_family = "unix")]
const RTLD_NOW: i32 = 2;

#[cfg(not(target_family = "unix"))]
pub fn register_ffi(_env: Env, _heap: &mut Heap) {}

#[cfg(target_family = "unix")]
pub fn register_ffi(env: Env, heap: &mut Heap) {
    env_set(
        heap,
        env,
        "lisp-dlopen".into(),
        Expr::Func(Rc::new(|args, _heap| {
            if args.len() != 1 {
                return Err("lisp-dlopen: expects exactly 1 argument (path)".into());
            }
            let path = match &args[0] {
                Expr::Str(s) => CString::new(s.as_str()),
                Expr::Symbol(s) => CString::new(s.as_str()),
                other => {
                    return Err(format!(
                        "lisp-dlopen: path must be a string, got {:?}",
                        other
                    ))
                }
            }
            .map_err(|_| "lisp-dlopen: path contains null byte".to_string())?;

            let handle = unsafe { dlopen(path.as_ptr(), RTLD_NOW) };
            if handle.is_null() {
                return Err("lisp-dlopen: failed to load library".into());
            }
            Ok(Expr::Int(handle as i64))
        })),
    );

    env_set(
        heap,
        env,
        "lisp-dlsym".into(),
        Expr::Func(Rc::new(|args, _heap| {
            if args.len() != 2 {
                return Err("lisp-dlsym: expects exactly 2 arguments (handle name)".into());
            }
            let handle = match &args[0] {
                Expr::Int(n) => *n as *mut u8,
                other => {
                    return Err(format!(
                        "lisp-dlsym: handle must be an integer (from lisp-dlopen), got {:?}",
                        other
                    ))
                }
            };
            let name = match &args[1] {
                Expr::Str(s) => CString::new(s.as_str()),
                Expr::Symbol(s) => CString::new(s.as_str()),
                other => {
                    return Err(format!(
                        "lisp-dlsym: name must be a string, got {:?}",
                        other
                    ))
                }
            }
            .map_err(|_| "lisp-dlsym: name contains null byte".to_string())?;

            let ptr = unsafe { dlsym(handle, name.as_ptr()) };
            if ptr.is_null() {
                return Err(format!("lisp-dlsym: symbol not found: {:?}", args[1]));
            }
            Ok(Expr::Int(ptr as i64))
        })),
    );

    env_set(
        heap,
        env,
        "lisp-dlclose".into(),
        Expr::Func(Rc::new(|args, _heap| {
            if args.len() != 1 {
                return Err("lisp-dlclose: expects exactly 1 argument (handle)".into());
            }
            let handle = match &args[0] {
                Expr::Int(n) => *n as *mut u8,
                other => {
                    return Err(format!(
                        "lisp-dlclose: handle must be an integer, got {:?}",
                        other
                    ))
                }
            };
            unsafe {
                dlclose(handle);
            }
            Ok(Expr::List(vec![]))
        })),
    );

    env_set(
        heap,
        env,
        "ccall".into(),
        Expr::Func(Rc::new(|args, _heap| {
            if args.len() < 2 {
                return Err("ccall: expects at least 2 arguments (fn ptr arg ...)".into());
            }
            let fn_ptr = match &args[0] {
                Expr::Int(n) => *n as *const (),
                other => {
                    return Err(format!(
                        "ccall: first argument must be a function pointer (integer), got {:?}",
                        other
                    ))
                }
            };

            let nargs = args.len() - 1;
            if nargs > 6 {
                return Err("ccall: supports at most 6 arguments (SysV ABI limit)".into());
            }

            let all_float = args[1..]
                .iter()
                .all(|a| matches!(a, Expr::Float(_)));

            if all_float {
                let mut fargs = [0.0f64; 6];
                for (i, arg) in args[1..].iter().enumerate() {
                    fargs[i] = match arg {
                        Expr::Float(f) => *f,
                        Expr::Int(n) => *n as f64,
                        other => {
                            return Err(format!(
                                "ccall: unsupported argument type in float mode: {:?}",
                                other
                            ))
                        }
                    };
                }
                let result = unsafe {
                    match nargs {
                        0 => {
                            let f: unsafe extern "C" fn() -> f64 = std::mem::transmute(fn_ptr);
                            f()
                        }
                        1 => {
                            let f: unsafe extern "C" fn(f64) -> f64 = std::mem::transmute(fn_ptr);
                            f(fargs[0])
                        }
                        2 => {
                            let f: unsafe extern "C" fn(f64, f64) -> f64 =
                                std::mem::transmute(fn_ptr);
                            f(fargs[0], fargs[1])
                        }
                        3 => {
                            let f: unsafe extern "C" fn(f64, f64, f64) -> f64 =
                                std::mem::transmute(fn_ptr);
                            f(fargs[0], fargs[1], fargs[2])
                        }
                        4 => {
                            let f: unsafe extern "C" fn(f64, f64, f64, f64) -> f64 =
                                std::mem::transmute(fn_ptr);
                            f(fargs[0], fargs[1], fargs[2], fargs[3])
                        }
                        5 => {
                            let f: unsafe extern "C" fn(f64, f64, f64, f64, f64) -> f64 =
                                std::mem::transmute(fn_ptr);
                            f(fargs[0], fargs[1], fargs[2], fargs[3], fargs[4])
                        }
                        6 => {
                            let f: unsafe extern "C" fn(f64, f64, f64, f64, f64, f64) -> f64 =
                                std::mem::transmute(fn_ptr);
                            f(fargs[0], fargs[1], fargs[2], fargs[3], fargs[4], fargs[5])
                        }
                        _ => unreachable!(),
                    }
                };
                Ok(Expr::Float(result))
            } else {
                let mut iargs = [0i64; 6];
                for (i, arg) in args[1..].iter().enumerate() {
                    iargs[i] = match arg {
                        Expr::Int(n) => *n,
                        Expr::Float(f) => *f as i64,
                        Expr::Bool(b) => {
                            if *b { 1 } else { 0 }
                        }
                        Expr::Str(s) => CString::new(s.as_str())
                            .map_err(|_| "ccall: string arg contains null byte".to_string())
                            .map(|c| c.into_raw() as i64)?,
                        Expr::Symbol(s) => CString::new(s.as_str())
                            .map_err(|_| "ccall: symbol arg contains null byte".to_string())
                            .map(|c| c.into_raw() as i64)?,
                        other => {
                            return Err(format!(
                                "ccall: unsupported argument type: {:?}",
                                other
                            ))
                        }
                    };
                }

                let result = unsafe {
                    match nargs {
                        0 => {
                            let f: unsafe extern "C" fn() -> i64 = std::mem::transmute(fn_ptr);
                            f()
                        }
                        1 => {
                            let f: unsafe extern "C" fn(i64) -> i64 = std::mem::transmute(fn_ptr);
                            f(iargs[0])
                        }
                        2 => {
                            let f: unsafe extern "C" fn(i64, i64) -> i64 =
                                std::mem::transmute(fn_ptr);
                            f(iargs[0], iargs[1])
                        }
                        3 => {
                            let f: unsafe extern "C" fn(i64, i64, i64) -> i64 =
                                std::mem::transmute(fn_ptr);
                            f(iargs[0], iargs[1], iargs[2])
                        }
                        4 => {
                            let f: unsafe extern "C" fn(i64, i64, i64, i64) -> i64 =
                                std::mem::transmute(fn_ptr);
                            f(iargs[0], iargs[1], iargs[2], iargs[3])
                        }
                        5 => {
                            let f: unsafe extern "C" fn(i64, i64, i64, i64, i64) -> i64 =
                                std::mem::transmute(fn_ptr);
                            f(iargs[0], iargs[1], iargs[2], iargs[3], iargs[4])
                        }
                        6 => {
                            let f: unsafe extern "C" fn(i64, i64, i64, i64, i64, i64) -> i64 =
                                std::mem::transmute(fn_ptr);
                            f(iargs[0], iargs[1], iargs[2], iargs[3], iargs[4], iargs[5])
                        }
                        _ => unreachable!(),
                    }
                };
                Ok(Expr::Int(result))
            }
        })),
    );
}
