#![allow(clippy::missing_transmute_annotations)]

use std::ffi::CString;
use std::mem::transmute;
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

#[cfg(not(target_family = "unix"))]
pub(crate) fn ccall_impl(_args: &[Expr]) -> Result<Expr, String> {
    Err("ccall is only supported on Unix".into())
}

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
        Expr::Func(Rc::new(|args, _heap| ccall_impl(args))),
    );

    // ── Memory primitives ──────────────────────────────────────────────────────
    //
    // These let users allocate, read, and write raw C-compatible memory,
    // enabling struct interop via `defstruct` + `ccall`.

    env_set(
        heap,
        env,
        "malloc".into(),
        Expr::Func(Rc::new(|args, _heap| {
            if args.len() != 1 {
                return Err("malloc: expects 1 argument (size in bytes)".into());
            }
            let size = match &args[0] {
                Expr::Int(n) => *n,
                other => return Err(format!("malloc: size must be an integer, got {:?}", other)),
            };
            if size <= 0 {
                return Err("malloc: size must be positive".into());
            }
            let ptr = unsafe { libc::malloc(size as usize) };
            if ptr.is_null() {
                return Err("malloc: allocation failed".into());
            }
            Ok(Expr::Int(ptr as i64))
        })),
    );

    env_set(
        heap,
        env,
        "free".into(),
        Expr::Func(Rc::new(|args, _heap| {
            if args.len() != 1 {
                return Err("free: expects 1 argument (pointer)".into());
            }
            let ptr = match &args[0] {
                Expr::Int(n) => *n as *mut libc::c_void,
                other => return Err(format!("free: pointer must be an integer, got {:?}", other)),
            };
            unsafe { libc::free(ptr) };
            Ok(Expr::List(vec![]))
        })),
    );

    env_set(
        heap,
        env,
        "mem-ref".into(),
        Expr::Func(Rc::new(|args, _heap| mem_ref_impl(args))),
    );

    env_set(
        heap,
        env,
        "mem-set!".into(),
        Expr::Func(Rc::new(|args, _heap| mem_set_impl(args))),
    );
}

// ── mem-ref implementation ─────────────────────────────────────────────────────

/// Read a value from raw memory: `(mem-ref ptr offset type)`.
fn mem_ref_impl(args: &[Expr]) -> Result<Expr, String> {
    if args.len() != 3 {
        return Err("mem-ref: expects 3 arguments (ptr offset type)".into());
    }
    let ptr = match &args[0] {
        Expr::Int(n) => *n as *const u8,
        other => return Err(format!("mem-ref: ptr must be an integer, got {:?}", other)),
    };
    let offset = match &args[1] {
        Expr::Int(n) => *n as usize,
        other => return Err(format!("mem-ref: offset must be an integer, got {:?}", other)),
    };
    let type_sym = match &args[2] {
        Expr::Symbol(s) => s.as_str(),
        other => return Err(format!("mem-ref: type must be a symbol, got {:?}", other)),
    };

    let addr = unsafe { ptr.add(offset) };

    match type_sym {
        ":int8" | ":i8" => {
            let val = unsafe { std::ptr::read_unaligned(addr as *const i8) };
            Ok(Expr::Int(val as i64))
        }
        ":uint8" | ":u8" => {
            let val = unsafe { std::ptr::read_unaligned(addr) };
            Ok(Expr::Int(val as i64))
        }
        ":int16" | ":i16" => {
            let val = unsafe { std::ptr::read_unaligned(addr as *const i16) };
            Ok(Expr::Int(val as i64))
        }
        ":uint16" | ":u16" => {
            let val = unsafe { std::ptr::read_unaligned(addr as *const u16) };
            Ok(Expr::Int(val as i64))
        }
        ":int32" | ":i32" | ":int" => {
            let val = unsafe { std::ptr::read_unaligned(addr as *const i32) };
            Ok(Expr::Int(val as i64))
        }
        ":uint32" | ":u32" => {
            let val = unsafe { std::ptr::read_unaligned(addr as *const u32) };
            Ok(Expr::Int(val as i64))
        }
        ":int64" | ":i64" => {
            let val = unsafe { std::ptr::read_unaligned(addr as *const i64) };
            Ok(Expr::Int(val))
        }
        ":uint64" | ":u64" => {
            let val = unsafe { std::ptr::read_unaligned(addr as *const u64) };
            Ok(Expr::Int(val as i64))
        }
        ":float" | ":f32" => {
            let val = unsafe { std::ptr::read_unaligned(addr as *const f32) };
            Ok(Expr::Float(val as f64))
        }
        ":double" | ":f64" | ":float64" => {
            let val = unsafe { std::ptr::read_unaligned(addr as *const f64) };
            Ok(Expr::Float(val))
        }
        ":ptr" => {
            let val = unsafe { std::ptr::read_unaligned(addr as *const *const u8) };
            Ok(Expr::Int(val as i64))
        }
        other => Err(format!("mem-ref: unknown type '{}'", other)),
    }
}

// ── mem-set! implementation ────────────────────────────────────────────────────

/// Write a value to raw memory: `(mem-set! ptr offset type value)`.
fn mem_set_impl(args: &[Expr]) -> Result<Expr, String> {
    if args.len() != 4 {
        return Err("mem-set!: expects 4 arguments (ptr offset type value)".into());
    }
    let ptr = match &args[0] {
        Expr::Int(n) => *n as *mut u8,
        other => return Err(format!("mem-set!: ptr must be an integer, got {:?}", other)),
    };
    let offset = match &args[1] {
        Expr::Int(n) => *n as usize,
        other => return Err(format!("mem-set!: offset must be an integer, got {:?}", other)),
    };
    let type_sym = match &args[2] {
        Expr::Symbol(s) => s.as_str(),
        other => return Err(format!("mem-set!: type must be a symbol, got {:?}", other)),
    };

    let addr = unsafe { ptr.add(offset) };

    match type_sym {
        ":int8" | ":i8" => {
            let val = as_i64(&args[3])? as i8;
            unsafe { std::ptr::write_unaligned(addr as *mut i8, val) };
            Ok(Expr::List(vec![]))
        }
        ":uint8" | ":u8" => {
            let val = as_i64(&args[3])? as u8;
            unsafe { std::ptr::write_unaligned(addr, val) };
            Ok(Expr::List(vec![]))
        }
        ":int16" | ":i16" => {
            let val = as_i64(&args[3])? as i16;
            unsafe { std::ptr::write_unaligned(addr as *mut i16, val) };
            Ok(Expr::List(vec![]))
        }
        ":uint16" | ":u16" => {
            let val = as_i64(&args[3])? as u16;
            unsafe { std::ptr::write_unaligned(addr as *mut u16, val) };
            Ok(Expr::List(vec![]))
        }
        ":int32" | ":i32" | ":int" => {
            let val = as_i64(&args[3])? as i32;
            unsafe { std::ptr::write_unaligned(addr as *mut i32, val) };
            Ok(Expr::List(vec![]))
        }
        ":uint32" | ":u32" => {
            let val = as_i64(&args[3])? as u32;
            unsafe { std::ptr::write_unaligned(addr as *mut u32, val) };
            Ok(Expr::List(vec![]))
        }
        ":int64" | ":i64" => {
            let val = as_i64(&args[3])?;
            unsafe { std::ptr::write_unaligned(addr as *mut i64, val) };
            Ok(Expr::List(vec![]))
        }
        ":uint64" | ":u64" => {
            let val = as_i64(&args[3])? as u64;
            unsafe { std::ptr::write_unaligned(addr as *mut u64, val) };
            Ok(Expr::List(vec![]))
        }
        ":float" | ":f32" => {
            let val = as_f64(&args[3])? as f32;
            unsafe { std::ptr::write_unaligned(addr as *mut f32, val) };
            Ok(Expr::List(vec![]))
        }
        ":double" | ":f64" | ":float64" => {
            let val = as_f64(&args[3])?;
            unsafe { std::ptr::write_unaligned(addr as *mut f64, val) };
            Ok(Expr::List(vec![]))
        }
        ":ptr" => {
            let val = as_i64(&args[3])? as *mut u8;
            unsafe { std::ptr::write_unaligned(addr as *mut *mut u8, val) };
            Ok(Expr::List(vec![]))
        }
        other => Err(format!("mem-set!: unknown type '{}'", other)),
    }
}

// ── Return type ──────────────────────────────────────────────────────────────

#[derive(Clone, Copy, PartialEq)]
enum RetType { Int, Float, Void, Ptr }

/// Parse an explicit return-type keyword from `args[1]`.
/// Returns `(ret_type, arg_offset, was_explicit)`.
fn parse_ret_type(args: &[Expr]) -> Result<(RetType, usize, bool), String> {
    if args.len() < 2 {
        return Ok((RetType::Int, 1, false));
    }
    match &args[1] {
        Expr::Symbol(s) if s == ":int" => Ok((RetType::Int, 2, true)),
        Expr::Symbol(s) if s == ":float" => Ok((RetType::Float, 2, true)),
        Expr::Symbol(s) if s == ":void" => Ok((RetType::Void, 2, true)),
        Expr::Symbol(s) if s == ":ptr" => Ok((RetType::Ptr, 2, true)),
        _ => Ok((RetType::Int, 1, false)),
    }
}

// ── Argument coercions ───────────────────────────────────────────────────────

fn as_i64(e: &Expr) -> Result<i64, String> {
    match e {
        Expr::Int(n) => Ok(*n),
        Expr::Float(f) => Ok(*f as i64),
        Expr::Bool(b) => Ok(if *b { 1 } else { 0 }),
        other => Err(format!("ccall: expected numeric value, got {:?}", other)),
    }
}

fn as_f64(e: &Expr) -> Result<f64, String> {
    match e {
        Expr::Float(f) => Ok(*f),
        Expr::Int(n) => Ok(*n as f64),
        Expr::Bool(b) => Ok(if *b { 1.0 } else { 0.0 }),
        other => Err(format!("ccall: expected numeric value, got {:?}", other)),
    }
}

fn cstring_ptr(s: &str, cstrings: &mut Vec<CString>) -> Result<i64, String> {
    let c = CString::new(s)
        .map_err(|_| "ccall: string argument contains null byte".to_string())?;
    let ptr = c.as_ptr() as i64;
    cstrings.push(c);
    Ok(ptr)
}

// ── ccall implementation ─────────────────────────────────────────────────────

pub(crate) fn ccall_impl(args: &[Expr]) -> Result<Expr, String> {
    if args.is_empty() {
        return Err("ccall: expects at least 1 argument (function pointer)".into());
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

    let (ret_type, arg_offset, ret_explicit) = parse_ret_type(args)?;
    let ncall_args = args.len().saturating_sub(arg_offset);
    if ncall_args > 6 {
        return Err("ccall: supports at most 6 arguments".into());
    }

    // ── Marshal arguments into int & float arrays ────────────────────────
    let mut int_args = [0i64; 6];
    let mut float_args = [0.0f64; 8];
    let mut pattern: u8 = 0; // bit i = 1 → arg i is float
    let mut int_idx = 0usize;
    let mut float_idx = 0usize;
    let mut cstrings: Vec<CString> = Vec::new();

    for i in 0..ncall_args {
        let arg = &args[arg_offset + i];

        // Check for typed arg: (type value) as list
        let typed = match arg {
            Expr::List(list) if list.len() == 2 => {
                if let Expr::Symbol(ty) = &list[0] {
                    Some((ty.as_str(), &list[1]))
                } else {
                    None
                }
            }
            _ => None,
        };

        if let Some((type_str, val)) = typed {
            match type_str {
                ":int" | ":ptr" => {
                    int_args[int_idx] = as_i64(val)?;
                    int_idx += 1;
                }
                ":float" => {
                    float_args[float_idx] = as_f64(val)?;
                    pattern |= 1 << i;
                    float_idx += 1;
                }
                ":str" | ":string" => {
                    let s = match val {
                        Expr::Str(s) => s.as_str(),
                        Expr::Symbol(s) => s.as_str(),
                        other => {
                            return Err(format!(
                                "ccall: :str arg value must be a string, got {:?}",
                                other
                            ))
                        }
                    };
                    int_args[int_idx] = cstring_ptr(s, &mut cstrings)?;
                    int_idx += 1;
                }
                ":bool" => {
                    let b = match val {
                        Expr::Bool(b) => *b,
                        other => {
                            return Err(format!(
                                "ccall: :bool arg value must be a bool, got {:?}",
                                other
                            ))
                        }
                    };
                    int_args[int_idx] = if b { 1 } else { 0 };
                    int_idx += 1;
                }
                _ => {
                    return Err(format!("ccall: unknown type specifier {}", type_str))
                }
            }
        } else {
            // ── Infer type from Expr variant ─────────────────────────────────
            match arg {
                Expr::Int(n) => {
                    int_args[int_idx] = *n;
                    int_idx += 1;
                }
                Expr::Float(f) => {
                    float_args[float_idx] = *f;
                    pattern |= 1 << i;
                    float_idx += 1;
                }
                Expr::Bool(b) => {
                    int_args[int_idx] = if *b { 1 } else { 0 };
                    int_idx += 1;
                }
                Expr::Str(s) => {
                    int_args[int_idx] = cstring_ptr(s.as_str(), &mut cstrings)?;
                    int_idx += 1;
                }
                Expr::Symbol(s) => {
                    int_args[int_idx] = cstring_ptr(s.as_str(), &mut cstrings)?;
                    int_idx += 1;
                }
                other => {
                    return Err(format!(
                        "ccall: unsupported argument type: {:?}",
                        other
                    ))
                }
            }
        }
    }

    // ── Determine return type ───────────────────────────────────────
    // When an explicit keyword was given, respect it.
    // When inferred, backward compat: all-float args → float return, else int return.
    let (ret_is_float, is_void_ret) = if ret_explicit {
        match ret_type {
            RetType::Float => (true, false),
            RetType::Void => (false, true),
            RetType::Int | RetType::Ptr => (false, false),
        }
    } else if float_idx > 0 && int_idx == 0 {
        // All arguments are float → infer float return
        (true, false)
    } else {
        (false, false)
    };

    // ── Make the call ───────────────────────────────────────────────────
    unsafe {
        if ncall_args == 0 {
            if is_void_ret {
                let f: unsafe extern "C" fn() = transmute(fn_ptr);
                f();
                Ok(Expr::List(vec![]))
            } else if ret_is_float {
                let f: unsafe extern "C" fn() -> f64 = transmute(fn_ptr);
                Ok(Expr::Float(f()))
            } else {
                let f: unsafe extern "C" fn() -> i64 = transmute(fn_ptr);
                Ok(Expr::Int(f()))
            }
        } else if float_idx == 0 {
            call_all_int(fn_ptr, ncall_args, &int_args, ret_is_float, ret_type)
        } else if int_idx == 0 {
            call_all_float(fn_ptr, ncall_args, &float_args, ret_is_float, ret_type)
        } else if ncall_args <= 6 {
            call_mixed(fn_ptr, ncall_args, pattern, &int_args, &float_args, ret_is_float, ret_type)
        } else {
            Err(
                "ccall: mixed int/float arguments only supports at most 6 arguments".into(),
            )
        }
    }
}

// ── All-int dispatch ─────────────────────────────────────────────────────────

fn call_all_int(
    fn_ptr: *const (),
    n: usize,
    iargs: &[i64; 6],
    ret_float: bool,
    ret_type: RetType,
) -> Result<Expr, String> {
    unsafe {
        if ret_type == RetType::Void {
            match n {
                1 => transmute::<_, extern "C" fn(i64)>(fn_ptr)(iargs[0]),
                2 => transmute::<_, extern "C" fn(i64, i64)>(fn_ptr)(iargs[0], iargs[1]),
                3 => transmute::<_, extern "C" fn(i64, i64, i64)>(fn_ptr)(iargs[0], iargs[1], iargs[2]),
                4 => transmute::<_, extern "C" fn(i64, i64, i64, i64)>(fn_ptr)(iargs[0], iargs[1], iargs[2], iargs[3]),
                5 => transmute::<_, extern "C" fn(i64, i64, i64, i64, i64)>(fn_ptr)(iargs[0], iargs[1], iargs[2], iargs[3], iargs[4]),
                6 => transmute::<_, extern "C" fn(i64, i64, i64, i64, i64, i64)>(fn_ptr)(iargs[0], iargs[1], iargs[2], iargs[3], iargs[4], iargs[5]),
                _ => unreachable!(),
            }
            return Ok(Expr::List(vec![]));
        }
        let r = match n {
            1 => transmute::<_, extern "C" fn(i64) -> i64>(fn_ptr)(iargs[0]),
            2 => transmute::<_, extern "C" fn(i64, i64) -> i64>(fn_ptr)(iargs[0], iargs[1]),
            3 => transmute::<_, extern "C" fn(i64, i64, i64) -> i64>(fn_ptr)(iargs[0], iargs[1], iargs[2]),
            4 => transmute::<_, extern "C" fn(i64, i64, i64, i64) -> i64>(fn_ptr)(iargs[0], iargs[1], iargs[2], iargs[3]),
            5 => transmute::<_, extern "C" fn(i64, i64, i64, i64, i64) -> i64>(fn_ptr)(iargs[0], iargs[1], iargs[2], iargs[3], iargs[4]),
            6 => transmute::<_, extern "C" fn(i64, i64, i64, i64, i64, i64) -> i64>(fn_ptr)(iargs[0], iargs[1], iargs[2], iargs[3], iargs[4], iargs[5]),
            _ => unreachable!(),
        };
        Ok(if ret_float { Expr::Float(r as f64) } else { Expr::Int(r) })
    }
}

// ── All-float dispatch ───────────────────────────────────────────────────────

fn call_all_float(
    fn_ptr: *const (),
    n: usize,
    fargs: &[f64; 8],
    ret_float: bool,
    ret_type: RetType,
) -> Result<Expr, String> {
    unsafe {
        if ret_type == RetType::Void {
            match n {
                1 => transmute::<_, extern "C" fn(f64)>(fn_ptr)(fargs[0]),
                2 => transmute::<_, extern "C" fn(f64, f64)>(fn_ptr)(fargs[0], fargs[1]),
                3 => transmute::<_, extern "C" fn(f64, f64, f64)>(fn_ptr)(fargs[0], fargs[1], fargs[2]),
                4 => transmute::<_, extern "C" fn(f64, f64, f64, f64)>(fn_ptr)(fargs[0], fargs[1], fargs[2], fargs[3]),
                5 => transmute::<_, extern "C" fn(f64, f64, f64, f64, f64)>(fn_ptr)(fargs[0], fargs[1], fargs[2], fargs[3], fargs[4]),
                6 => transmute::<_, extern "C" fn(f64, f64, f64, f64, f64, f64)>(fn_ptr)(fargs[0], fargs[1], fargs[2], fargs[3], fargs[4], fargs[5]),
                _ => unreachable!(),
            }
            return Ok(Expr::List(vec![]));
        }
        if ret_float {
            let r = match n {
                1 => transmute::<_, extern "C" fn(f64) -> f64>(fn_ptr)(fargs[0]),
                2 => transmute::<_, extern "C" fn(f64, f64) -> f64>(fn_ptr)(fargs[0], fargs[1]),
                3 => transmute::<_, extern "C" fn(f64, f64, f64) -> f64>(fn_ptr)(fargs[0], fargs[1], fargs[2]),
                4 => transmute::<_, extern "C" fn(f64, f64, f64, f64) -> f64>(fn_ptr)(fargs[0], fargs[1], fargs[2], fargs[3]),
                5 => transmute::<_, extern "C" fn(f64, f64, f64, f64, f64) -> f64>(fn_ptr)(fargs[0], fargs[1], fargs[2], fargs[3], fargs[4]),
                6 => transmute::<_, extern "C" fn(f64, f64, f64, f64, f64, f64) -> f64>(fn_ptr)(fargs[0], fargs[1], fargs[2], fargs[3], fargs[4], fargs[5]),
                _ => unreachable!(),
            };
            Ok(Expr::Float(r))
        } else {
            let r = match n {
                1 => transmute::<_, extern "C" fn(f64) -> i64>(fn_ptr)(fargs[0]),
                2 => transmute::<_, extern "C" fn(f64, f64) -> i64>(fn_ptr)(fargs[0], fargs[1]),
                3 => transmute::<_, extern "C" fn(f64, f64, f64) -> i64>(fn_ptr)(fargs[0], fargs[1], fargs[2]),
                4 => transmute::<_, extern "C" fn(f64, f64, f64, f64) -> i64>(fn_ptr)(fargs[0], fargs[1], fargs[2], fargs[3]),
                5 => transmute::<_, extern "C" fn(f64, f64, f64, f64, f64) -> i64>(fn_ptr)(fargs[0], fargs[1], fargs[2], fargs[3], fargs[4]),
                6 => transmute::<_, extern "C" fn(f64, f64, f64, f64, f64, f64) -> i64>(fn_ptr)(fargs[0], fargs[1], fargs[2], fargs[3], fargs[4], fargs[5]),
                _ => unreachable!(),
            };
            Ok(Expr::Int(r))
        }
    }
}

// ── Mixed-type dispatch (1–4 args, ABI-correct across all platforms) ───────
// Each argument is independently classified as INTEGER (i64) or SSE (f64).
// Bit `i` of `pattern` is 1 when argument `i` is float.
// int_args / float_args hold the marshaled values in encounter order.
//
// Rust's `extern "C" fn(…)` generates the correct per-architecture ABI for
// the exact argument type sequence — so a transmute to a function signature
// matching the C function's actual prototype will pass each argument in the
// correct register or stack slot on any platform.

fn call_mixed(
    fn_ptr: *const (),
    n: usize,
    pattern: u8,
    iargs: &[i64; 6],
    fargs: &[f64; 8],
    ret_float: bool,
    ret_type: RetType,
) -> Result<Expr, String> {
    // Handle void return at the top so the per-arity functions are simpler.
    if ret_type == RetType::Void {
        match n {
            1 => call_mixed_void_1(fn_ptr, pattern, iargs, fargs),
            2 => call_mixed_void_2(fn_ptr, pattern, iargs, fargs),
            3 => call_mixed_void_3(fn_ptr, pattern, iargs, fargs),
            4 => call_mixed_void_4(fn_ptr, pattern, iargs, fargs),
            5 => call_mixed_void_5(fn_ptr, pattern, iargs, fargs),
            6 => call_mixed_void_6(fn_ptr, pattern, iargs, fargs),
            _ => unreachable!(),
        }
        return Ok(Expr::List(vec![]));
    }
    match n {
        1 => call_mixed_1(fn_ptr, pattern, iargs, fargs, ret_float),
        2 => call_mixed_2(fn_ptr, pattern, iargs, fargs, ret_float),
        3 => call_mixed_3(fn_ptr, pattern, iargs, fargs, ret_float),
        4 => call_mixed_4(fn_ptr, pattern, iargs, fargs, ret_float),
        5 => call_mixed_5(fn_ptr, pattern, iargs, fargs, ret_float),
        6 => call_mixed_6(fn_ptr, pattern, iargs, fargs, ret_float),
        _ => unreachable!(),
    }
}

// ── Void mixed helpers ───────────────────────────────────────────────────────

fn call_mixed_void_1(
    fn_ptr: *const (), pattern: u8,
    iargs: &[i64; 6], fargs: &[f64; 8],
) {
    unsafe {
        match pattern & 1 {
            0 => transmute::<_, extern "C" fn(i64)>(fn_ptr)(iargs[0]),
            1 => transmute::<_, extern "C" fn(f64)>(fn_ptr)(fargs[0]),
            _ => unreachable!(),
        }
    }
}

fn call_mixed_void_2(
    fn_ptr: *const (), pattern: u8,
    iargs: &[i64; 6], fargs: &[f64; 8],
) {
    unsafe {
        match pattern & 3 {
            0b00 => transmute::<_, extern "C" fn(i64, i64)>(fn_ptr)(iargs[0], iargs[1]),
            0b01 => transmute::<_, extern "C" fn(f64, i64)>(fn_ptr)(fargs[0], iargs[0]),
            0b10 => transmute::<_, extern "C" fn(i64, f64)>(fn_ptr)(iargs[0], fargs[0]),
            0b11 => transmute::<_, extern "C" fn(f64, f64)>(fn_ptr)(fargs[0], fargs[1]),
            _ => unreachable!(),
        }
    }
}

fn call_mixed_void_3(
    fn_ptr: *const (), pattern: u8,
    iargs: &[i64; 6], fargs: &[f64; 8],
) {
    unsafe {
        match pattern & 7 {
            0b000 => transmute::<_, extern "C" fn(i64, i64, i64)>(fn_ptr)(iargs[0], iargs[1], iargs[2]),
            0b001 => transmute::<_, extern "C" fn(f64, i64, i64)>(fn_ptr)(fargs[0], iargs[0], iargs[1]),
            0b010 => transmute::<_, extern "C" fn(i64, f64, i64)>(fn_ptr)(iargs[0], fargs[0], iargs[1]),
            0b011 => transmute::<_, extern "C" fn(f64, f64, i64)>(fn_ptr)(fargs[0], fargs[1], iargs[0]),
            0b100 => transmute::<_, extern "C" fn(i64, i64, f64)>(fn_ptr)(iargs[0], iargs[1], fargs[0]),
            0b101 => transmute::<_, extern "C" fn(f64, i64, f64)>(fn_ptr)(fargs[0], iargs[0], fargs[1]),
            0b110 => transmute::<_, extern "C" fn(i64, f64, f64)>(fn_ptr)(iargs[0], fargs[0], fargs[1]),
            0b111 => transmute::<_, extern "C" fn(f64, f64, f64)>(fn_ptr)(fargs[0], fargs[1], fargs[2]),
            _ => unreachable!(),
        }
    }
}

fn call_mixed_void_4(
    fn_ptr: *const (), pattern: u8,
    iargs: &[i64; 6], fargs: &[f64; 8],
) {
    unsafe {
        match pattern & 15 {
            0b0000 => transmute::<_, extern "C" fn(i64, i64, i64, i64)>(fn_ptr)(iargs[0], iargs[1], iargs[2], iargs[3]),
            0b0001 => transmute::<_, extern "C" fn(f64, i64, i64, i64)>(fn_ptr)(fargs[0], iargs[0], iargs[1], iargs[2]),
            0b0010 => transmute::<_, extern "C" fn(i64, f64, i64, i64)>(fn_ptr)(iargs[0], fargs[0], iargs[1], iargs[2]),
            0b0011 => transmute::<_, extern "C" fn(f64, f64, i64, i64)>(fn_ptr)(fargs[0], fargs[1], iargs[0], iargs[1]),
            0b0100 => transmute::<_, extern "C" fn(i64, i64, f64, i64)>(fn_ptr)(iargs[0], iargs[1], fargs[0], iargs[2]),
            0b0101 => transmute::<_, extern "C" fn(f64, i64, f64, i64)>(fn_ptr)(fargs[0], iargs[0], fargs[1], iargs[1]),
            0b0110 => transmute::<_, extern "C" fn(i64, f64, f64, i64)>(fn_ptr)(iargs[0], fargs[0], fargs[1], iargs[1]),
            0b0111 => transmute::<_, extern "C" fn(f64, f64, f64, i64)>(fn_ptr)(fargs[0], fargs[1], fargs[2], iargs[0]),
            0b1000 => transmute::<_, extern "C" fn(i64, i64, i64, f64)>(fn_ptr)(iargs[0], iargs[1], iargs[2], fargs[0]),
            0b1001 => transmute::<_, extern "C" fn(f64, i64, i64, f64)>(fn_ptr)(fargs[0], iargs[0], iargs[1], fargs[1]),
            0b1010 => transmute::<_, extern "C" fn(i64, f64, i64, f64)>(fn_ptr)(iargs[0], fargs[0], iargs[1], fargs[1]),
            0b1011 => transmute::<_, extern "C" fn(f64, f64, i64, f64)>(fn_ptr)(fargs[0], fargs[1], iargs[0], fargs[2]),
            0b1100 => transmute::<_, extern "C" fn(i64, i64, f64, f64)>(fn_ptr)(iargs[0], iargs[1], fargs[0], fargs[1]),
            0b1101 => transmute::<_, extern "C" fn(f64, i64, f64, f64)>(fn_ptr)(fargs[0], iargs[0], fargs[1], fargs[2]),
            0b1110 => transmute::<_, extern "C" fn(i64, f64, f64, f64)>(fn_ptr)(iargs[0], fargs[0], fargs[1], fargs[2]),
            0b1111 => transmute::<_, extern "C" fn(f64, f64, f64, f64)>(fn_ptr)(fargs[0], fargs[1], fargs[2], fargs[3]),
            _ => unreachable!(),
        }
    }
}
fn call_mixed_void_5(
    fn_ptr: *const (), pattern: u8,
    iargs: &[i64; 6], fargs: &[f64; 8],
) {
    unsafe {
        match pattern & 31 {
            0b00000 => transmute::<_, extern "C" fn(i64, i64, i64, i64, i64)>(fn_ptr)(iargs[0], iargs[1], iargs[2], iargs[3], iargs[4]),
            0b00001 => transmute::<_, extern "C" fn(f64, i64, i64, i64, i64)>(fn_ptr)(fargs[0], iargs[0], iargs[1], iargs[2], iargs[3]),
            0b00010 => transmute::<_, extern "C" fn(i64, f64, i64, i64, i64)>(fn_ptr)(iargs[0], fargs[0], iargs[1], iargs[2], iargs[3]),
            0b00011 => transmute::<_, extern "C" fn(f64, f64, i64, i64, i64)>(fn_ptr)(fargs[0], fargs[1], iargs[0], iargs[1], iargs[2]),
            0b00100 => transmute::<_, extern "C" fn(i64, i64, f64, i64, i64)>(fn_ptr)(iargs[0], iargs[1], fargs[0], iargs[2], iargs[3]),
            0b00101 => transmute::<_, extern "C" fn(f64, i64, f64, i64, i64)>(fn_ptr)(fargs[0], iargs[0], fargs[1], iargs[1], iargs[2]),
            0b00110 => transmute::<_, extern "C" fn(i64, f64, f64, i64, i64)>(fn_ptr)(iargs[0], fargs[0], fargs[1], iargs[1], iargs[2]),
            0b00111 => transmute::<_, extern "C" fn(f64, f64, f64, i64, i64)>(fn_ptr)(fargs[0], fargs[1], fargs[2], iargs[0], iargs[1]),
            0b01000 => transmute::<_, extern "C" fn(i64, i64, i64, f64, i64)>(fn_ptr)(iargs[0], iargs[1], iargs[2], fargs[0], iargs[3]),
            0b01001 => transmute::<_, extern "C" fn(f64, i64, i64, f64, i64)>(fn_ptr)(fargs[0], iargs[0], iargs[1], fargs[1], iargs[2]),
            0b01010 => transmute::<_, extern "C" fn(i64, f64, i64, f64, i64)>(fn_ptr)(iargs[0], fargs[0], iargs[1], fargs[1], iargs[2]),
            0b01011 => transmute::<_, extern "C" fn(f64, f64, i64, f64, i64)>(fn_ptr)(fargs[0], fargs[1], iargs[0], fargs[2], iargs[1]),
            0b01100 => transmute::<_, extern "C" fn(i64, i64, f64, f64, i64)>(fn_ptr)(iargs[0], iargs[1], fargs[0], fargs[1], iargs[2]),
            0b01101 => transmute::<_, extern "C" fn(f64, i64, f64, f64, i64)>(fn_ptr)(fargs[0], iargs[0], fargs[1], fargs[2], iargs[1]),
            0b01110 => transmute::<_, extern "C" fn(i64, f64, f64, f64, i64)>(fn_ptr)(iargs[0], fargs[0], fargs[1], fargs[2], iargs[1]),
            0b01111 => transmute::<_, extern "C" fn(f64, f64, f64, f64, i64)>(fn_ptr)(fargs[0], fargs[1], fargs[2], fargs[3], iargs[0]),
            0b10000 => transmute::<_, extern "C" fn(i64, i64, i64, i64, f64)>(fn_ptr)(iargs[0], iargs[1], iargs[2], iargs[3], fargs[0]),
            0b10001 => transmute::<_, extern "C" fn(f64, i64, i64, i64, f64)>(fn_ptr)(fargs[0], iargs[0], iargs[1], iargs[2], fargs[1]),
            0b10010 => transmute::<_, extern "C" fn(i64, f64, i64, i64, f64)>(fn_ptr)(iargs[0], fargs[0], iargs[1], iargs[2], fargs[1]),
            0b10011 => transmute::<_, extern "C" fn(f64, f64, i64, i64, f64)>(fn_ptr)(fargs[0], fargs[1], iargs[0], iargs[1], fargs[2]),
            0b10100 => transmute::<_, extern "C" fn(i64, i64, f64, i64, f64)>(fn_ptr)(iargs[0], iargs[1], fargs[0], iargs[2], fargs[1]),
            0b10101 => transmute::<_, extern "C" fn(f64, i64, f64, i64, f64)>(fn_ptr)(fargs[0], iargs[0], fargs[1], iargs[1], fargs[2]),
            0b10110 => transmute::<_, extern "C" fn(i64, f64, f64, i64, f64)>(fn_ptr)(iargs[0], fargs[0], fargs[1], iargs[1], fargs[2]),
            0b10111 => transmute::<_, extern "C" fn(f64, f64, f64, i64, f64)>(fn_ptr)(fargs[0], fargs[1], fargs[2], iargs[0], fargs[3]),
            0b11000 => transmute::<_, extern "C" fn(i64, i64, i64, f64, f64)>(fn_ptr)(iargs[0], iargs[1], iargs[2], fargs[0], fargs[1]),
            0b11001 => transmute::<_, extern "C" fn(f64, i64, i64, f64, f64)>(fn_ptr)(fargs[0], iargs[0], iargs[1], fargs[1], fargs[2]),
            0b11010 => transmute::<_, extern "C" fn(i64, f64, i64, f64, f64)>(fn_ptr)(iargs[0], fargs[0], iargs[1], fargs[1], fargs[2]),
            0b11011 => transmute::<_, extern "C" fn(f64, f64, i64, f64, f64)>(fn_ptr)(fargs[0], fargs[1], iargs[0], fargs[2], fargs[3]),
            0b11100 => transmute::<_, extern "C" fn(i64, i64, f64, f64, f64)>(fn_ptr)(iargs[0], iargs[1], fargs[0], fargs[1], fargs[2]),
            0b11101 => transmute::<_, extern "C" fn(f64, i64, f64, f64, f64)>(fn_ptr)(fargs[0], iargs[0], fargs[1], fargs[2], fargs[3]),
            0b11110 => transmute::<_, extern "C" fn(i64, f64, f64, f64, f64)>(fn_ptr)(iargs[0], fargs[0], fargs[1], fargs[2], fargs[3]),
            0b11111 => transmute::<_, extern "C" fn(f64, f64, f64, f64, f64)>(fn_ptr)(fargs[0], fargs[1], fargs[2], fargs[3], fargs[4]),
            _ => unreachable!(),
        }
    }
}

fn call_mixed_void_6(
    fn_ptr: *const (), pattern: u8,
    iargs: &[i64; 6], fargs: &[f64; 8],
) {
    unsafe {
        match pattern & 63 {
            0b000000 => transmute::<_, extern "C" fn(i64, i64, i64, i64, i64, i64)>(fn_ptr)(iargs[0], iargs[1], iargs[2], iargs[3], iargs[4], iargs[5]),
            0b000001 => transmute::<_, extern "C" fn(f64, i64, i64, i64, i64, i64)>(fn_ptr)(fargs[0], iargs[0], iargs[1], iargs[2], iargs[3], iargs[4]),
            0b000010 => transmute::<_, extern "C" fn(i64, f64, i64, i64, i64, i64)>(fn_ptr)(iargs[0], fargs[0], iargs[1], iargs[2], iargs[3], iargs[4]),
            0b000011 => transmute::<_, extern "C" fn(f64, f64, i64, i64, i64, i64)>(fn_ptr)(fargs[0], fargs[1], iargs[0], iargs[1], iargs[2], iargs[3]),
            0b000100 => transmute::<_, extern "C" fn(i64, i64, f64, i64, i64, i64)>(fn_ptr)(iargs[0], iargs[1], fargs[0], iargs[2], iargs[3], iargs[4]),
            0b000101 => transmute::<_, extern "C" fn(f64, i64, f64, i64, i64, i64)>(fn_ptr)(fargs[0], iargs[0], fargs[1], iargs[1], iargs[2], iargs[3]),
            0b000110 => transmute::<_, extern "C" fn(i64, f64, f64, i64, i64, i64)>(fn_ptr)(iargs[0], fargs[0], fargs[1], iargs[1], iargs[2], iargs[3]),
            0b000111 => transmute::<_, extern "C" fn(f64, f64, f64, i64, i64, i64)>(fn_ptr)(fargs[0], fargs[1], fargs[2], iargs[0], iargs[1], iargs[2]),
            0b001000 => transmute::<_, extern "C" fn(i64, i64, i64, f64, i64, i64)>(fn_ptr)(iargs[0], iargs[1], iargs[2], fargs[0], iargs[3], iargs[4]),
            0b001001 => transmute::<_, extern "C" fn(f64, i64, i64, f64, i64, i64)>(fn_ptr)(fargs[0], iargs[0], iargs[1], fargs[1], iargs[2], iargs[3]),
            0b001010 => transmute::<_, extern "C" fn(i64, f64, i64, f64, i64, i64)>(fn_ptr)(iargs[0], fargs[0], iargs[1], fargs[1], iargs[2], iargs[3]),
            0b001011 => transmute::<_, extern "C" fn(f64, f64, i64, f64, i64, i64)>(fn_ptr)(fargs[0], fargs[1], iargs[0], fargs[2], iargs[1], iargs[2]),
            0b001100 => transmute::<_, extern "C" fn(i64, i64, f64, f64, i64, i64)>(fn_ptr)(iargs[0], iargs[1], fargs[0], fargs[1], iargs[2], iargs[3]),
            0b001101 => transmute::<_, extern "C" fn(f64, i64, f64, f64, i64, i64)>(fn_ptr)(fargs[0], iargs[0], fargs[1], fargs[2], iargs[1], iargs[2]),
            0b001110 => transmute::<_, extern "C" fn(i64, f64, f64, f64, i64, i64)>(fn_ptr)(iargs[0], fargs[0], fargs[1], fargs[2], iargs[1], iargs[2]),
            0b001111 => transmute::<_, extern "C" fn(f64, f64, f64, f64, i64, i64)>(fn_ptr)(fargs[0], fargs[1], fargs[2], fargs[3], iargs[0], iargs[1]),
            0b010000 => transmute::<_, extern "C" fn(i64, i64, i64, i64, f64, i64)>(fn_ptr)(iargs[0], iargs[1], iargs[2], iargs[3], fargs[0], iargs[4]),
            0b010001 => transmute::<_, extern "C" fn(f64, i64, i64, i64, f64, i64)>(fn_ptr)(fargs[0], iargs[0], iargs[1], iargs[2], fargs[1], iargs[3]),
            0b010010 => transmute::<_, extern "C" fn(i64, f64, i64, i64, f64, i64)>(fn_ptr)(iargs[0], fargs[0], iargs[1], iargs[2], fargs[1], iargs[3]),
            0b010011 => transmute::<_, extern "C" fn(f64, f64, i64, i64, f64, i64)>(fn_ptr)(fargs[0], fargs[1], iargs[0], iargs[1], fargs[2], iargs[2]),
            0b010100 => transmute::<_, extern "C" fn(i64, i64, f64, i64, f64, i64)>(fn_ptr)(iargs[0], iargs[1], fargs[0], iargs[2], fargs[1], iargs[3]),
            0b010101 => transmute::<_, extern "C" fn(f64, i64, f64, i64, f64, i64)>(fn_ptr)(fargs[0], iargs[0], fargs[1], iargs[1], fargs[2], iargs[2]),
            0b010110 => transmute::<_, extern "C" fn(i64, f64, f64, i64, f64, i64)>(fn_ptr)(iargs[0], fargs[0], fargs[1], iargs[1], fargs[2], iargs[2]),
            0b010111 => transmute::<_, extern "C" fn(f64, f64, f64, i64, f64, i64)>(fn_ptr)(fargs[0], fargs[1], fargs[2], iargs[0], fargs[3], iargs[1]),
            0b011000 => transmute::<_, extern "C" fn(i64, i64, i64, f64, f64, i64)>(fn_ptr)(iargs[0], iargs[1], iargs[2], fargs[0], fargs[1], iargs[3]),
            0b011001 => transmute::<_, extern "C" fn(f64, i64, i64, f64, f64, i64)>(fn_ptr)(fargs[0], iargs[0], iargs[1], fargs[1], fargs[2], iargs[2]),
            0b011010 => transmute::<_, extern "C" fn(i64, f64, i64, f64, f64, i64)>(fn_ptr)(iargs[0], fargs[0], iargs[1], fargs[1], fargs[2], iargs[2]),
            0b011011 => transmute::<_, extern "C" fn(f64, f64, i64, f64, f64, i64)>(fn_ptr)(fargs[0], fargs[1], iargs[0], fargs[2], fargs[3], iargs[1]),
            0b011100 => transmute::<_, extern "C" fn(i64, i64, f64, f64, f64, i64)>(fn_ptr)(iargs[0], iargs[1], fargs[0], fargs[1], fargs[2], iargs[2]),
            0b011101 => transmute::<_, extern "C" fn(f64, i64, f64, f64, f64, i64)>(fn_ptr)(fargs[0], iargs[0], fargs[1], fargs[2], fargs[3], iargs[1]),
            0b011110 => transmute::<_, extern "C" fn(i64, f64, f64, f64, f64, i64)>(fn_ptr)(iargs[0], fargs[0], fargs[1], fargs[2], fargs[3], iargs[1]),
            0b011111 => transmute::<_, extern "C" fn(f64, f64, f64, f64, f64, i64)>(fn_ptr)(fargs[0], fargs[1], fargs[2], fargs[3], fargs[4], iargs[0]),
            0b100000 => transmute::<_, extern "C" fn(i64, i64, i64, i64, i64, f64)>(fn_ptr)(iargs[0], iargs[1], iargs[2], iargs[3], iargs[4], fargs[0]),
            0b100001 => transmute::<_, extern "C" fn(f64, i64, i64, i64, i64, f64)>(fn_ptr)(fargs[0], iargs[0], iargs[1], iargs[2], iargs[3], fargs[1]),
            0b100010 => transmute::<_, extern "C" fn(i64, f64, i64, i64, i64, f64)>(fn_ptr)(iargs[0], fargs[0], iargs[1], iargs[2], iargs[3], fargs[1]),
            0b100011 => transmute::<_, extern "C" fn(f64, f64, i64, i64, i64, f64)>(fn_ptr)(fargs[0], fargs[1], iargs[0], iargs[1], iargs[2], fargs[2]),
            0b100100 => transmute::<_, extern "C" fn(i64, i64, f64, i64, i64, f64)>(fn_ptr)(iargs[0], iargs[1], fargs[0], iargs[2], iargs[3], fargs[1]),
            0b100101 => transmute::<_, extern "C" fn(f64, i64, f64, i64, i64, f64)>(fn_ptr)(fargs[0], iargs[0], fargs[1], iargs[1], iargs[2], fargs[2]),
            0b100110 => transmute::<_, extern "C" fn(i64, f64, f64, i64, i64, f64)>(fn_ptr)(iargs[0], fargs[0], fargs[1], iargs[1], iargs[2], fargs[2]),
            0b100111 => transmute::<_, extern "C" fn(f64, f64, f64, i64, i64, f64)>(fn_ptr)(fargs[0], fargs[1], fargs[2], iargs[0], iargs[1], fargs[3]),
            0b101000 => transmute::<_, extern "C" fn(i64, i64, i64, f64, i64, f64)>(fn_ptr)(iargs[0], iargs[1], iargs[2], fargs[0], iargs[3], fargs[1]),
            0b101001 => transmute::<_, extern "C" fn(f64, i64, i64, f64, i64, f64)>(fn_ptr)(fargs[0], iargs[0], iargs[1], fargs[1], iargs[2], fargs[2]),
            0b101010 => transmute::<_, extern "C" fn(i64, f64, i64, f64, i64, f64)>(fn_ptr)(iargs[0], fargs[0], iargs[1], fargs[1], iargs[2], fargs[2]),
            0b101011 => transmute::<_, extern "C" fn(f64, f64, i64, f64, i64, f64)>(fn_ptr)(fargs[0], fargs[1], iargs[0], fargs[2], iargs[1], fargs[3]),
            0b101100 => transmute::<_, extern "C" fn(i64, i64, f64, f64, i64, f64)>(fn_ptr)(iargs[0], iargs[1], fargs[0], fargs[1], iargs[2], fargs[2]),
            0b101101 => transmute::<_, extern "C" fn(f64, i64, f64, f64, i64, f64)>(fn_ptr)(fargs[0], iargs[0], fargs[1], fargs[2], iargs[1], fargs[3]),
            0b101110 => transmute::<_, extern "C" fn(i64, f64, f64, f64, i64, f64)>(fn_ptr)(iargs[0], fargs[0], fargs[1], fargs[2], iargs[1], fargs[3]),
            0b101111 => transmute::<_, extern "C" fn(f64, f64, f64, f64, i64, f64)>(fn_ptr)(fargs[0], fargs[1], fargs[2], fargs[3], iargs[0], fargs[4]),
            0b110000 => transmute::<_, extern "C" fn(i64, i64, i64, i64, f64, f64)>(fn_ptr)(iargs[0], iargs[1], iargs[2], iargs[3], fargs[0], fargs[1]),
            0b110001 => transmute::<_, extern "C" fn(f64, i64, i64, i64, f64, f64)>(fn_ptr)(fargs[0], iargs[0], iargs[1], iargs[2], fargs[1], fargs[2]),
            0b110010 => transmute::<_, extern "C" fn(i64, f64, i64, i64, f64, f64)>(fn_ptr)(iargs[0], fargs[0], iargs[1], iargs[2], fargs[1], fargs[2]),
            0b110011 => transmute::<_, extern "C" fn(f64, f64, i64, i64, f64, f64)>(fn_ptr)(fargs[0], fargs[1], iargs[0], iargs[1], fargs[2], fargs[3]),
            0b110100 => transmute::<_, extern "C" fn(i64, i64, f64, i64, f64, f64)>(fn_ptr)(iargs[0], iargs[1], fargs[0], iargs[2], fargs[1], fargs[2]),
            0b110101 => transmute::<_, extern "C" fn(f64, i64, f64, i64, f64, f64)>(fn_ptr)(fargs[0], iargs[0], fargs[1], iargs[1], fargs[2], fargs[3]),
            0b110110 => transmute::<_, extern "C" fn(i64, f64, f64, i64, f64, f64)>(fn_ptr)(iargs[0], fargs[0], fargs[1], iargs[1], fargs[2], fargs[3]),
            0b110111 => transmute::<_, extern "C" fn(f64, f64, f64, i64, f64, f64)>(fn_ptr)(fargs[0], fargs[1], fargs[2], iargs[0], fargs[3], fargs[4]),
            0b111000 => transmute::<_, extern "C" fn(i64, i64, i64, f64, f64, f64)>(fn_ptr)(iargs[0], iargs[1], iargs[2], fargs[0], fargs[1], fargs[2]),
            0b111001 => transmute::<_, extern "C" fn(f64, i64, i64, f64, f64, f64)>(fn_ptr)(fargs[0], iargs[0], iargs[1], fargs[1], fargs[2], fargs[3]),
            0b111010 => transmute::<_, extern "C" fn(i64, f64, i64, f64, f64, f64)>(fn_ptr)(iargs[0], fargs[0], iargs[1], fargs[1], fargs[2], fargs[3]),
            0b111011 => transmute::<_, extern "C" fn(f64, f64, i64, f64, f64, f64)>(fn_ptr)(fargs[0], fargs[1], iargs[0], fargs[2], fargs[3], fargs[4]),
            0b111100 => transmute::<_, extern "C" fn(i64, i64, f64, f64, f64, f64)>(fn_ptr)(iargs[0], iargs[1], fargs[0], fargs[1], fargs[2], fargs[3]),
            0b111101 => transmute::<_, extern "C" fn(f64, i64, f64, f64, f64, f64)>(fn_ptr)(fargs[0], iargs[0], fargs[1], fargs[2], fargs[3], fargs[4]),
            0b111110 => transmute::<_, extern "C" fn(i64, f64, f64, f64, f64, f64)>(fn_ptr)(iargs[0], fargs[0], fargs[1], fargs[2], fargs[3], fargs[4]),
            0b111111 => transmute::<_, extern "C" fn(f64, f64, f64, f64, f64, f64)>(fn_ptr)(fargs[0], fargs[1], fargs[2], fargs[3], fargs[4], fargs[5]),
            _ => unreachable!(),
        }
    }
}


fn call_mixed_5(
    fn_ptr: *const (), pattern: u8,
    iargs: &[i64; 6], fargs: &[f64; 8],
    ret_float: bool,
) -> Result<Expr, String> {
    unsafe {
        match (pattern & 31, ret_float) {
            (0b00000, false) => Ok(Expr::Int(transmute::<_, extern "C" fn(i64, i64, i64, i64, i64) -> i64>(fn_ptr)(iargs[0], iargs[1], iargs[2], iargs[3], iargs[4]))),
            (0b00000, true)  => Ok(Expr::Float(transmute::<_, extern "C" fn(i64, i64, i64, i64, i64) -> f64>(fn_ptr)(iargs[0], iargs[1], iargs[2], iargs[3], iargs[4]))),
            (0b00001, false) => Ok(Expr::Int(transmute::<_, extern "C" fn(f64, i64, i64, i64, i64) -> i64>(fn_ptr)(fargs[0], iargs[0], iargs[1], iargs[2], iargs[3]))),
            (0b00001, true)  => Ok(Expr::Float(transmute::<_, extern "C" fn(f64, i64, i64, i64, i64) -> f64>(fn_ptr)(fargs[0], iargs[0], iargs[1], iargs[2], iargs[3]))),
            (0b00010, false) => Ok(Expr::Int(transmute::<_, extern "C" fn(i64, f64, i64, i64, i64) -> i64>(fn_ptr)(iargs[0], fargs[0], iargs[1], iargs[2], iargs[3]))),
            (0b00010, true)  => Ok(Expr::Float(transmute::<_, extern "C" fn(i64, f64, i64, i64, i64) -> f64>(fn_ptr)(iargs[0], fargs[0], iargs[1], iargs[2], iargs[3]))),
            (0b00011, false) => Ok(Expr::Int(transmute::<_, extern "C" fn(f64, f64, i64, i64, i64) -> i64>(fn_ptr)(fargs[0], fargs[1], iargs[0], iargs[1], iargs[2]))),
            (0b00011, true)  => Ok(Expr::Float(transmute::<_, extern "C" fn(f64, f64, i64, i64, i64) -> f64>(fn_ptr)(fargs[0], fargs[1], iargs[0], iargs[1], iargs[2]))),
            (0b00100, false) => Ok(Expr::Int(transmute::<_, extern "C" fn(i64, i64, f64, i64, i64) -> i64>(fn_ptr)(iargs[0], iargs[1], fargs[0], iargs[2], iargs[3]))),
            (0b00100, true)  => Ok(Expr::Float(transmute::<_, extern "C" fn(i64, i64, f64, i64, i64) -> f64>(fn_ptr)(iargs[0], iargs[1], fargs[0], iargs[2], iargs[3]))),
            (0b00101, false) => Ok(Expr::Int(transmute::<_, extern "C" fn(f64, i64, f64, i64, i64) -> i64>(fn_ptr)(fargs[0], iargs[0], fargs[1], iargs[1], iargs[2]))),
            (0b00101, true)  => Ok(Expr::Float(transmute::<_, extern "C" fn(f64, i64, f64, i64, i64) -> f64>(fn_ptr)(fargs[0], iargs[0], fargs[1], iargs[1], iargs[2]))),
            (0b00110, false) => Ok(Expr::Int(transmute::<_, extern "C" fn(i64, f64, f64, i64, i64) -> i64>(fn_ptr)(iargs[0], fargs[0], fargs[1], iargs[1], iargs[2]))),
            (0b00110, true)  => Ok(Expr::Float(transmute::<_, extern "C" fn(i64, f64, f64, i64, i64) -> f64>(fn_ptr)(iargs[0], fargs[0], fargs[1], iargs[1], iargs[2]))),
            (0b00111, false) => Ok(Expr::Int(transmute::<_, extern "C" fn(f64, f64, f64, i64, i64) -> i64>(fn_ptr)(fargs[0], fargs[1], fargs[2], iargs[0], iargs[1]))),
            (0b00111, true)  => Ok(Expr::Float(transmute::<_, extern "C" fn(f64, f64, f64, i64, i64) -> f64>(fn_ptr)(fargs[0], fargs[1], fargs[2], iargs[0], iargs[1]))),
            (0b01000, false) => Ok(Expr::Int(transmute::<_, extern "C" fn(i64, i64, i64, f64, i64) -> i64>(fn_ptr)(iargs[0], iargs[1], iargs[2], fargs[0], iargs[3]))),
            (0b01000, true)  => Ok(Expr::Float(transmute::<_, extern "C" fn(i64, i64, i64, f64, i64) -> f64>(fn_ptr)(iargs[0], iargs[1], iargs[2], fargs[0], iargs[3]))),
            (0b01001, false) => Ok(Expr::Int(transmute::<_, extern "C" fn(f64, i64, i64, f64, i64) -> i64>(fn_ptr)(fargs[0], iargs[0], iargs[1], fargs[1], iargs[2]))),
            (0b01001, true)  => Ok(Expr::Float(transmute::<_, extern "C" fn(f64, i64, i64, f64, i64) -> f64>(fn_ptr)(fargs[0], iargs[0], iargs[1], fargs[1], iargs[2]))),
            (0b01010, false) => Ok(Expr::Int(transmute::<_, extern "C" fn(i64, f64, i64, f64, i64) -> i64>(fn_ptr)(iargs[0], fargs[0], iargs[1], fargs[1], iargs[2]))),
            (0b01010, true)  => Ok(Expr::Float(transmute::<_, extern "C" fn(i64, f64, i64, f64, i64) -> f64>(fn_ptr)(iargs[0], fargs[0], iargs[1], fargs[1], iargs[2]))),
            (0b01011, false) => Ok(Expr::Int(transmute::<_, extern "C" fn(f64, f64, i64, f64, i64) -> i64>(fn_ptr)(fargs[0], fargs[1], iargs[0], fargs[2], iargs[1]))),
            (0b01011, true)  => Ok(Expr::Float(transmute::<_, extern "C" fn(f64, f64, i64, f64, i64) -> f64>(fn_ptr)(fargs[0], fargs[1], iargs[0], fargs[2], iargs[1]))),
            (0b01100, false) => Ok(Expr::Int(transmute::<_, extern "C" fn(i64, i64, f64, f64, i64) -> i64>(fn_ptr)(iargs[0], iargs[1], fargs[0], fargs[1], iargs[2]))),
            (0b01100, true)  => Ok(Expr::Float(transmute::<_, extern "C" fn(i64, i64, f64, f64, i64) -> f64>(fn_ptr)(iargs[0], iargs[1], fargs[0], fargs[1], iargs[2]))),
            (0b01101, false) => Ok(Expr::Int(transmute::<_, extern "C" fn(f64, i64, f64, f64, i64) -> i64>(fn_ptr)(fargs[0], iargs[0], fargs[1], fargs[2], iargs[1]))),
            (0b01101, true)  => Ok(Expr::Float(transmute::<_, extern "C" fn(f64, i64, f64, f64, i64) -> f64>(fn_ptr)(fargs[0], iargs[0], fargs[1], fargs[2], iargs[1]))),
            (0b01110, false) => Ok(Expr::Int(transmute::<_, extern "C" fn(i64, f64, f64, f64, i64) -> i64>(fn_ptr)(iargs[0], fargs[0], fargs[1], fargs[2], iargs[1]))),
            (0b01110, true)  => Ok(Expr::Float(transmute::<_, extern "C" fn(i64, f64, f64, f64, i64) -> f64>(fn_ptr)(iargs[0], fargs[0], fargs[1], fargs[2], iargs[1]))),
            (0b01111, false) => Ok(Expr::Int(transmute::<_, extern "C" fn(f64, f64, f64, f64, i64) -> i64>(fn_ptr)(fargs[0], fargs[1], fargs[2], fargs[3], iargs[0]))),
            (0b01111, true)  => Ok(Expr::Float(transmute::<_, extern "C" fn(f64, f64, f64, f64, i64) -> f64>(fn_ptr)(fargs[0], fargs[1], fargs[2], fargs[3], iargs[0]))),
            (0b10000, false) => Ok(Expr::Int(transmute::<_, extern "C" fn(i64, i64, i64, i64, f64) -> i64>(fn_ptr)(iargs[0], iargs[1], iargs[2], iargs[3], fargs[0]))),
            (0b10000, true)  => Ok(Expr::Float(transmute::<_, extern "C" fn(i64, i64, i64, i64, f64) -> f64>(fn_ptr)(iargs[0], iargs[1], iargs[2], iargs[3], fargs[0]))),
            (0b10001, false) => Ok(Expr::Int(transmute::<_, extern "C" fn(f64, i64, i64, i64, f64) -> i64>(fn_ptr)(fargs[0], iargs[0], iargs[1], iargs[2], fargs[1]))),
            (0b10001, true)  => Ok(Expr::Float(transmute::<_, extern "C" fn(f64, i64, i64, i64, f64) -> f64>(fn_ptr)(fargs[0], iargs[0], iargs[1], iargs[2], fargs[1]))),
            (0b10010, false) => Ok(Expr::Int(transmute::<_, extern "C" fn(i64, f64, i64, i64, f64) -> i64>(fn_ptr)(iargs[0], fargs[0], iargs[1], iargs[2], fargs[1]))),
            (0b10010, true)  => Ok(Expr::Float(transmute::<_, extern "C" fn(i64, f64, i64, i64, f64) -> f64>(fn_ptr)(iargs[0], fargs[0], iargs[1], iargs[2], fargs[1]))),
            (0b10011, false) => Ok(Expr::Int(transmute::<_, extern "C" fn(f64, f64, i64, i64, f64) -> i64>(fn_ptr)(fargs[0], fargs[1], iargs[0], iargs[1], fargs[2]))),
            (0b10011, true)  => Ok(Expr::Float(transmute::<_, extern "C" fn(f64, f64, i64, i64, f64) -> f64>(fn_ptr)(fargs[0], fargs[1], iargs[0], iargs[1], fargs[2]))),
            (0b10100, false) => Ok(Expr::Int(transmute::<_, extern "C" fn(i64, i64, f64, i64, f64) -> i64>(fn_ptr)(iargs[0], iargs[1], fargs[0], iargs[2], fargs[1]))),
            (0b10100, true)  => Ok(Expr::Float(transmute::<_, extern "C" fn(i64, i64, f64, i64, f64) -> f64>(fn_ptr)(iargs[0], iargs[1], fargs[0], iargs[2], fargs[1]))),
            (0b10101, false) => Ok(Expr::Int(transmute::<_, extern "C" fn(f64, i64, f64, i64, f64) -> i64>(fn_ptr)(fargs[0], iargs[0], fargs[1], iargs[1], fargs[2]))),
            (0b10101, true)  => Ok(Expr::Float(transmute::<_, extern "C" fn(f64, i64, f64, i64, f64) -> f64>(fn_ptr)(fargs[0], iargs[0], fargs[1], iargs[1], fargs[2]))),
            (0b10110, false) => Ok(Expr::Int(transmute::<_, extern "C" fn(i64, f64, f64, i64, f64) -> i64>(fn_ptr)(iargs[0], fargs[0], fargs[1], iargs[1], fargs[2]))),
            (0b10110, true)  => Ok(Expr::Float(transmute::<_, extern "C" fn(i64, f64, f64, i64, f64) -> f64>(fn_ptr)(iargs[0], fargs[0], fargs[1], iargs[1], fargs[2]))),
            (0b10111, false) => Ok(Expr::Int(transmute::<_, extern "C" fn(f64, f64, f64, i64, f64) -> i64>(fn_ptr)(fargs[0], fargs[1], fargs[2], iargs[0], fargs[3]))),
            (0b10111, true)  => Ok(Expr::Float(transmute::<_, extern "C" fn(f64, f64, f64, i64, f64) -> f64>(fn_ptr)(fargs[0], fargs[1], fargs[2], iargs[0], fargs[3]))),
            (0b11000, false) => Ok(Expr::Int(transmute::<_, extern "C" fn(i64, i64, i64, f64, f64) -> i64>(fn_ptr)(iargs[0], iargs[1], iargs[2], fargs[0], fargs[1]))),
            (0b11000, true)  => Ok(Expr::Float(transmute::<_, extern "C" fn(i64, i64, i64, f64, f64) -> f64>(fn_ptr)(iargs[0], iargs[1], iargs[2], fargs[0], fargs[1]))),
            (0b11001, false) => Ok(Expr::Int(transmute::<_, extern "C" fn(f64, i64, i64, f64, f64) -> i64>(fn_ptr)(fargs[0], iargs[0], iargs[1], fargs[1], fargs[2]))),
            (0b11001, true)  => Ok(Expr::Float(transmute::<_, extern "C" fn(f64, i64, i64, f64, f64) -> f64>(fn_ptr)(fargs[0], iargs[0], iargs[1], fargs[1], fargs[2]))),
            (0b11010, false) => Ok(Expr::Int(transmute::<_, extern "C" fn(i64, f64, i64, f64, f64) -> i64>(fn_ptr)(iargs[0], fargs[0], iargs[1], fargs[1], fargs[2]))),
            (0b11010, true)  => Ok(Expr::Float(transmute::<_, extern "C" fn(i64, f64, i64, f64, f64) -> f64>(fn_ptr)(iargs[0], fargs[0], iargs[1], fargs[1], fargs[2]))),
            (0b11011, false) => Ok(Expr::Int(transmute::<_, extern "C" fn(f64, f64, i64, f64, f64) -> i64>(fn_ptr)(fargs[0], fargs[1], iargs[0], fargs[2], fargs[3]))),
            (0b11011, true)  => Ok(Expr::Float(transmute::<_, extern "C" fn(f64, f64, i64, f64, f64) -> f64>(fn_ptr)(fargs[0], fargs[1], iargs[0], fargs[2], fargs[3]))),
            (0b11100, false) => Ok(Expr::Int(transmute::<_, extern "C" fn(i64, i64, f64, f64, f64) -> i64>(fn_ptr)(iargs[0], iargs[1], fargs[0], fargs[1], fargs[2]))),
            (0b11100, true)  => Ok(Expr::Float(transmute::<_, extern "C" fn(i64, i64, f64, f64, f64) -> f64>(fn_ptr)(iargs[0], iargs[1], fargs[0], fargs[1], fargs[2]))),
            (0b11101, false) => Ok(Expr::Int(transmute::<_, extern "C" fn(f64, i64, f64, f64, f64) -> i64>(fn_ptr)(fargs[0], iargs[0], fargs[1], fargs[2], fargs[3]))),
            (0b11101, true)  => Ok(Expr::Float(transmute::<_, extern "C" fn(f64, i64, f64, f64, f64) -> f64>(fn_ptr)(fargs[0], iargs[0], fargs[1], fargs[2], fargs[3]))),
            (0b11110, false) => Ok(Expr::Int(transmute::<_, extern "C" fn(i64, f64, f64, f64, f64) -> i64>(fn_ptr)(iargs[0], fargs[0], fargs[1], fargs[2], fargs[3]))),
            (0b11110, true)  => Ok(Expr::Float(transmute::<_, extern "C" fn(i64, f64, f64, f64, f64) -> f64>(fn_ptr)(iargs[0], fargs[0], fargs[1], fargs[2], fargs[3]))),
            (0b11111, false) => Ok(Expr::Int(transmute::<_, extern "C" fn(f64, f64, f64, f64, f64) -> i64>(fn_ptr)(fargs[0], fargs[1], fargs[2], fargs[3], fargs[4]))),
            (0b11111, true)  => Ok(Expr::Float(transmute::<_, extern "C" fn(f64, f64, f64, f64, f64) -> f64>(fn_ptr)(fargs[0], fargs[1], fargs[2], fargs[3], fargs[4]))),
            _ => unreachable!(),
        }
    }
}

fn call_mixed_6(
    fn_ptr: *const (), pattern: u8,
    iargs: &[i64; 6], fargs: &[f64; 8],
    ret_float: bool,
) -> Result<Expr, String> {
    unsafe {
        match (pattern & 63, ret_float) {
            (0b000000, false) => Ok(Expr::Int(transmute::<_, extern "C" fn(i64, i64, i64, i64, i64, i64) -> i64>(fn_ptr)(iargs[0], iargs[1], iargs[2], iargs[3], iargs[4], iargs[5]))),
            (0b000000, true)  => Ok(Expr::Float(transmute::<_, extern "C" fn(i64, i64, i64, i64, i64, i64) -> f64>(fn_ptr)(iargs[0], iargs[1], iargs[2], iargs[3], iargs[4], iargs[5]))),
            (0b000001, false) => Ok(Expr::Int(transmute::<_, extern "C" fn(f64, i64, i64, i64, i64, i64) -> i64>(fn_ptr)(fargs[0], iargs[0], iargs[1], iargs[2], iargs[3], iargs[4]))),
            (0b000001, true)  => Ok(Expr::Float(transmute::<_, extern "C" fn(f64, i64, i64, i64, i64, i64) -> f64>(fn_ptr)(fargs[0], iargs[0], iargs[1], iargs[2], iargs[3], iargs[4]))),
            (0b000010, false) => Ok(Expr::Int(transmute::<_, extern "C" fn(i64, f64, i64, i64, i64, i64) -> i64>(fn_ptr)(iargs[0], fargs[0], iargs[1], iargs[2], iargs[3], iargs[4]))),
            (0b000010, true)  => Ok(Expr::Float(transmute::<_, extern "C" fn(i64, f64, i64, i64, i64, i64) -> f64>(fn_ptr)(iargs[0], fargs[0], iargs[1], iargs[2], iargs[3], iargs[4]))),
            (0b000011, false) => Ok(Expr::Int(transmute::<_, extern "C" fn(f64, f64, i64, i64, i64, i64) -> i64>(fn_ptr)(fargs[0], fargs[1], iargs[0], iargs[1], iargs[2], iargs[3]))),
            (0b000011, true)  => Ok(Expr::Float(transmute::<_, extern "C" fn(f64, f64, i64, i64, i64, i64) -> f64>(fn_ptr)(fargs[0], fargs[1], iargs[0], iargs[1], iargs[2], iargs[3]))),
            (0b000100, false) => Ok(Expr::Int(transmute::<_, extern "C" fn(i64, i64, f64, i64, i64, i64) -> i64>(fn_ptr)(iargs[0], iargs[1], fargs[0], iargs[2], iargs[3], iargs[4]))),
            (0b000100, true)  => Ok(Expr::Float(transmute::<_, extern "C" fn(i64, i64, f64, i64, i64, i64) -> f64>(fn_ptr)(iargs[0], iargs[1], fargs[0], iargs[2], iargs[3], iargs[4]))),
            (0b000101, false) => Ok(Expr::Int(transmute::<_, extern "C" fn(f64, i64, f64, i64, i64, i64) -> i64>(fn_ptr)(fargs[0], iargs[0], fargs[1], iargs[1], iargs[2], iargs[3]))),
            (0b000101, true)  => Ok(Expr::Float(transmute::<_, extern "C" fn(f64, i64, f64, i64, i64, i64) -> f64>(fn_ptr)(fargs[0], iargs[0], fargs[1], iargs[1], iargs[2], iargs[3]))),
            (0b000110, false) => Ok(Expr::Int(transmute::<_, extern "C" fn(i64, f64, f64, i64, i64, i64) -> i64>(fn_ptr)(iargs[0], fargs[0], fargs[1], iargs[1], iargs[2], iargs[3]))),
            (0b000110, true)  => Ok(Expr::Float(transmute::<_, extern "C" fn(i64, f64, f64, i64, i64, i64) -> f64>(fn_ptr)(iargs[0], fargs[0], fargs[1], iargs[1], iargs[2], iargs[3]))),
            (0b000111, false) => Ok(Expr::Int(transmute::<_, extern "C" fn(f64, f64, f64, i64, i64, i64) -> i64>(fn_ptr)(fargs[0], fargs[1], fargs[2], iargs[0], iargs[1], iargs[2]))),
            (0b000111, true)  => Ok(Expr::Float(transmute::<_, extern "C" fn(f64, f64, f64, i64, i64, i64) -> f64>(fn_ptr)(fargs[0], fargs[1], fargs[2], iargs[0], iargs[1], iargs[2]))),
            (0b001000, false) => Ok(Expr::Int(transmute::<_, extern "C" fn(i64, i64, i64, f64, i64, i64) -> i64>(fn_ptr)(iargs[0], iargs[1], iargs[2], fargs[0], iargs[3], iargs[4]))),
            (0b001000, true)  => Ok(Expr::Float(transmute::<_, extern "C" fn(i64, i64, i64, f64, i64, i64) -> f64>(fn_ptr)(iargs[0], iargs[1], iargs[2], fargs[0], iargs[3], iargs[4]))),
            (0b001001, false) => Ok(Expr::Int(transmute::<_, extern "C" fn(f64, i64, i64, f64, i64, i64) -> i64>(fn_ptr)(fargs[0], iargs[0], iargs[1], fargs[1], iargs[2], iargs[3]))),
            (0b001001, true)  => Ok(Expr::Float(transmute::<_, extern "C" fn(f64, i64, i64, f64, i64, i64) -> f64>(fn_ptr)(fargs[0], iargs[0], iargs[1], fargs[1], iargs[2], iargs[3]))),
            (0b001010, false) => Ok(Expr::Int(transmute::<_, extern "C" fn(i64, f64, i64, f64, i64, i64) -> i64>(fn_ptr)(iargs[0], fargs[0], iargs[1], fargs[1], iargs[2], iargs[3]))),
            (0b001010, true)  => Ok(Expr::Float(transmute::<_, extern "C" fn(i64, f64, i64, f64, i64, i64) -> f64>(fn_ptr)(iargs[0], fargs[0], iargs[1], fargs[1], iargs[2], iargs[3]))),
            (0b001011, false) => Ok(Expr::Int(transmute::<_, extern "C" fn(f64, f64, i64, f64, i64, i64) -> i64>(fn_ptr)(fargs[0], fargs[1], iargs[0], fargs[2], iargs[1], iargs[2]))),
            (0b001011, true)  => Ok(Expr::Float(transmute::<_, extern "C" fn(f64, f64, i64, f64, i64, i64) -> f64>(fn_ptr)(fargs[0], fargs[1], iargs[0], fargs[2], iargs[1], iargs[2]))),
            (0b001100, false) => Ok(Expr::Int(transmute::<_, extern "C" fn(i64, i64, f64, f64, i64, i64) -> i64>(fn_ptr)(iargs[0], iargs[1], fargs[0], fargs[1], iargs[2], iargs[3]))),
            (0b001100, true)  => Ok(Expr::Float(transmute::<_, extern "C" fn(i64, i64, f64, f64, i64, i64) -> f64>(fn_ptr)(iargs[0], iargs[1], fargs[0], fargs[1], iargs[2], iargs[3]))),
            (0b001101, false) => Ok(Expr::Int(transmute::<_, extern "C" fn(f64, i64, f64, f64, i64, i64) -> i64>(fn_ptr)(fargs[0], iargs[0], fargs[1], fargs[2], iargs[1], iargs[2]))),
            (0b001101, true)  => Ok(Expr::Float(transmute::<_, extern "C" fn(f64, i64, f64, f64, i64, i64) -> f64>(fn_ptr)(fargs[0], iargs[0], fargs[1], fargs[2], iargs[1], iargs[2]))),
            (0b001110, false) => Ok(Expr::Int(transmute::<_, extern "C" fn(i64, f64, f64, f64, i64, i64) -> i64>(fn_ptr)(iargs[0], fargs[0], fargs[1], fargs[2], iargs[1], iargs[2]))),
            (0b001110, true)  => Ok(Expr::Float(transmute::<_, extern "C" fn(i64, f64, f64, f64, i64, i64) -> f64>(fn_ptr)(iargs[0], fargs[0], fargs[1], fargs[2], iargs[1], iargs[2]))),
            (0b001111, false) => Ok(Expr::Int(transmute::<_, extern "C" fn(f64, f64, f64, f64, i64, i64) -> i64>(fn_ptr)(fargs[0], fargs[1], fargs[2], fargs[3], iargs[0], iargs[1]))),
            (0b001111, true)  => Ok(Expr::Float(transmute::<_, extern "C" fn(f64, f64, f64, f64, i64, i64) -> f64>(fn_ptr)(fargs[0], fargs[1], fargs[2], fargs[3], iargs[0], iargs[1]))),
            (0b010000, false) => Ok(Expr::Int(transmute::<_, extern "C" fn(i64, i64, i64, i64, f64, i64) -> i64>(fn_ptr)(iargs[0], iargs[1], iargs[2], iargs[3], fargs[0], iargs[4]))),
            (0b010000, true)  => Ok(Expr::Float(transmute::<_, extern "C" fn(i64, i64, i64, i64, f64, i64) -> f64>(fn_ptr)(iargs[0], iargs[1], iargs[2], iargs[3], fargs[0], iargs[4]))),
            (0b010001, false) => Ok(Expr::Int(transmute::<_, extern "C" fn(f64, i64, i64, i64, f64, i64) -> i64>(fn_ptr)(fargs[0], iargs[0], iargs[1], iargs[2], fargs[1], iargs[3]))),
            (0b010001, true)  => Ok(Expr::Float(transmute::<_, extern "C" fn(f64, i64, i64, i64, f64, i64) -> f64>(fn_ptr)(fargs[0], iargs[0], iargs[1], iargs[2], fargs[1], iargs[3]))),
            (0b010010, false) => Ok(Expr::Int(transmute::<_, extern "C" fn(i64, f64, i64, i64, f64, i64) -> i64>(fn_ptr)(iargs[0], fargs[0], iargs[1], iargs[2], fargs[1], iargs[3]))),
            (0b010010, true)  => Ok(Expr::Float(transmute::<_, extern "C" fn(i64, f64, i64, i64, f64, i64) -> f64>(fn_ptr)(iargs[0], fargs[0], iargs[1], iargs[2], fargs[1], iargs[3]))),
            (0b010011, false) => Ok(Expr::Int(transmute::<_, extern "C" fn(f64, f64, i64, i64, f64, i64) -> i64>(fn_ptr)(fargs[0], fargs[1], iargs[0], iargs[1], fargs[2], iargs[2]))),
            (0b010011, true)  => Ok(Expr::Float(transmute::<_, extern "C" fn(f64, f64, i64, i64, f64, i64) -> f64>(fn_ptr)(fargs[0], fargs[1], iargs[0], iargs[1], fargs[2], iargs[2]))),
            (0b010100, false) => Ok(Expr::Int(transmute::<_, extern "C" fn(i64, i64, f64, i64, f64, i64) -> i64>(fn_ptr)(iargs[0], iargs[1], fargs[0], iargs[2], fargs[1], iargs[3]))),
            (0b010100, true)  => Ok(Expr::Float(transmute::<_, extern "C" fn(i64, i64, f64, i64, f64, i64) -> f64>(fn_ptr)(iargs[0], iargs[1], fargs[0], iargs[2], fargs[1], iargs[3]))),
            (0b010101, false) => Ok(Expr::Int(transmute::<_, extern "C" fn(f64, i64, f64, i64, f64, i64) -> i64>(fn_ptr)(fargs[0], iargs[0], fargs[1], iargs[1], fargs[2], iargs[2]))),
            (0b010101, true)  => Ok(Expr::Float(transmute::<_, extern "C" fn(f64, i64, f64, i64, f64, i64) -> f64>(fn_ptr)(fargs[0], iargs[0], fargs[1], iargs[1], fargs[2], iargs[2]))),
            (0b010110, false) => Ok(Expr::Int(transmute::<_, extern "C" fn(i64, f64, f64, i64, f64, i64) -> i64>(fn_ptr)(iargs[0], fargs[0], fargs[1], iargs[1], fargs[2], iargs[2]))),
            (0b010110, true)  => Ok(Expr::Float(transmute::<_, extern "C" fn(i64, f64, f64, i64, f64, i64) -> f64>(fn_ptr)(iargs[0], fargs[0], fargs[1], iargs[1], fargs[2], iargs[2]))),
            (0b010111, false) => Ok(Expr::Int(transmute::<_, extern "C" fn(f64, f64, f64, i64, f64, i64) -> i64>(fn_ptr)(fargs[0], fargs[1], fargs[2], iargs[0], fargs[3], iargs[1]))),
            (0b010111, true)  => Ok(Expr::Float(transmute::<_, extern "C" fn(f64, f64, f64, i64, f64, i64) -> f64>(fn_ptr)(fargs[0], fargs[1], fargs[2], iargs[0], fargs[3], iargs[1]))),
            (0b011000, false) => Ok(Expr::Int(transmute::<_, extern "C" fn(i64, i64, i64, f64, f64, i64) -> i64>(fn_ptr)(iargs[0], iargs[1], iargs[2], fargs[0], fargs[1], iargs[3]))),
            (0b011000, true)  => Ok(Expr::Float(transmute::<_, extern "C" fn(i64, i64, i64, f64, f64, i64) -> f64>(fn_ptr)(iargs[0], iargs[1], iargs[2], fargs[0], fargs[1], iargs[3]))),
            (0b011001, false) => Ok(Expr::Int(transmute::<_, extern "C" fn(f64, i64, i64, f64, f64, i64) -> i64>(fn_ptr)(fargs[0], iargs[0], iargs[1], fargs[1], fargs[2], iargs[2]))),
            (0b011001, true)  => Ok(Expr::Float(transmute::<_, extern "C" fn(f64, i64, i64, f64, f64, i64) -> f64>(fn_ptr)(fargs[0], iargs[0], iargs[1], fargs[1], fargs[2], iargs[2]))),
            (0b011010, false) => Ok(Expr::Int(transmute::<_, extern "C" fn(i64, f64, i64, f64, f64, i64) -> i64>(fn_ptr)(iargs[0], fargs[0], iargs[1], fargs[1], fargs[2], iargs[2]))),
            (0b011010, true)  => Ok(Expr::Float(transmute::<_, extern "C" fn(i64, f64, i64, f64, f64, i64) -> f64>(fn_ptr)(iargs[0], fargs[0], iargs[1], fargs[1], fargs[2], iargs[2]))),
            (0b011011, false) => Ok(Expr::Int(transmute::<_, extern "C" fn(f64, f64, i64, f64, f64, i64) -> i64>(fn_ptr)(fargs[0], fargs[1], iargs[0], fargs[2], fargs[3], iargs[1]))),
            (0b011011, true)  => Ok(Expr::Float(transmute::<_, extern "C" fn(f64, f64, i64, f64, f64, i64) -> f64>(fn_ptr)(fargs[0], fargs[1], iargs[0], fargs[2], fargs[3], iargs[1]))),
            (0b011100, false) => Ok(Expr::Int(transmute::<_, extern "C" fn(i64, i64, f64, f64, f64, i64) -> i64>(fn_ptr)(iargs[0], iargs[1], fargs[0], fargs[1], fargs[2], iargs[2]))),
            (0b011100, true)  => Ok(Expr::Float(transmute::<_, extern "C" fn(i64, i64, f64, f64, f64, i64) -> f64>(fn_ptr)(iargs[0], iargs[1], fargs[0], fargs[1], fargs[2], iargs[2]))),
            (0b011101, false) => Ok(Expr::Int(transmute::<_, extern "C" fn(f64, i64, f64, f64, f64, i64) -> i64>(fn_ptr)(fargs[0], iargs[0], fargs[1], fargs[2], fargs[3], iargs[1]))),
            (0b011101, true)  => Ok(Expr::Float(transmute::<_, extern "C" fn(f64, i64, f64, f64, f64, i64) -> f64>(fn_ptr)(fargs[0], iargs[0], fargs[1], fargs[2], fargs[3], iargs[1]))),
            (0b011110, false) => Ok(Expr::Int(transmute::<_, extern "C" fn(i64, f64, f64, f64, f64, i64) -> i64>(fn_ptr)(iargs[0], fargs[0], fargs[1], fargs[2], fargs[3], iargs[1]))),
            (0b011110, true)  => Ok(Expr::Float(transmute::<_, extern "C" fn(i64, f64, f64, f64, f64, i64) -> f64>(fn_ptr)(iargs[0], fargs[0], fargs[1], fargs[2], fargs[3], iargs[1]))),
            (0b011111, false) => Ok(Expr::Int(transmute::<_, extern "C" fn(f64, f64, f64, f64, f64, i64) -> i64>(fn_ptr)(fargs[0], fargs[1], fargs[2], fargs[3], fargs[4], iargs[0]))),
            (0b011111, true)  => Ok(Expr::Float(transmute::<_, extern "C" fn(f64, f64, f64, f64, f64, i64) -> f64>(fn_ptr)(fargs[0], fargs[1], fargs[2], fargs[3], fargs[4], iargs[0]))),
            (0b100000, false) => Ok(Expr::Int(transmute::<_, extern "C" fn(i64, i64, i64, i64, i64, f64) -> i64>(fn_ptr)(iargs[0], iargs[1], iargs[2], iargs[3], iargs[4], fargs[0]))),
            (0b100000, true)  => Ok(Expr::Float(transmute::<_, extern "C" fn(i64, i64, i64, i64, i64, f64) -> f64>(fn_ptr)(iargs[0], iargs[1], iargs[2], iargs[3], iargs[4], fargs[0]))),
            (0b100001, false) => Ok(Expr::Int(transmute::<_, extern "C" fn(f64, i64, i64, i64, i64, f64) -> i64>(fn_ptr)(fargs[0], iargs[0], iargs[1], iargs[2], iargs[3], fargs[1]))),
            (0b100001, true)  => Ok(Expr::Float(transmute::<_, extern "C" fn(f64, i64, i64, i64, i64, f64) -> f64>(fn_ptr)(fargs[0], iargs[0], iargs[1], iargs[2], iargs[3], fargs[1]))),
            (0b100010, false) => Ok(Expr::Int(transmute::<_, extern "C" fn(i64, f64, i64, i64, i64, f64) -> i64>(fn_ptr)(iargs[0], fargs[0], iargs[1], iargs[2], iargs[3], fargs[1]))),
            (0b100010, true)  => Ok(Expr::Float(transmute::<_, extern "C" fn(i64, f64, i64, i64, i64, f64) -> f64>(fn_ptr)(iargs[0], fargs[0], iargs[1], iargs[2], iargs[3], fargs[1]))),
            (0b100011, false) => Ok(Expr::Int(transmute::<_, extern "C" fn(f64, f64, i64, i64, i64, f64) -> i64>(fn_ptr)(fargs[0], fargs[1], iargs[0], iargs[1], iargs[2], fargs[2]))),
            (0b100011, true)  => Ok(Expr::Float(transmute::<_, extern "C" fn(f64, f64, i64, i64, i64, f64) -> f64>(fn_ptr)(fargs[0], fargs[1], iargs[0], iargs[1], iargs[2], fargs[2]))),
            (0b100100, false) => Ok(Expr::Int(transmute::<_, extern "C" fn(i64, i64, f64, i64, i64, f64) -> i64>(fn_ptr)(iargs[0], iargs[1], fargs[0], iargs[2], iargs[3], fargs[1]))),
            (0b100100, true)  => Ok(Expr::Float(transmute::<_, extern "C" fn(i64, i64, f64, i64, i64, f64) -> f64>(fn_ptr)(iargs[0], iargs[1], fargs[0], iargs[2], iargs[3], fargs[1]))),
            (0b100101, false) => Ok(Expr::Int(transmute::<_, extern "C" fn(f64, i64, f64, i64, i64, f64) -> i64>(fn_ptr)(fargs[0], iargs[0], fargs[1], iargs[1], iargs[2], fargs[2]))),
            (0b100101, true)  => Ok(Expr::Float(transmute::<_, extern "C" fn(f64, i64, f64, i64, i64, f64) -> f64>(fn_ptr)(fargs[0], iargs[0], fargs[1], iargs[1], iargs[2], fargs[2]))),
            (0b100110, false) => Ok(Expr::Int(transmute::<_, extern "C" fn(i64, f64, f64, i64, i64, f64) -> i64>(fn_ptr)(iargs[0], fargs[0], fargs[1], iargs[1], iargs[2], fargs[2]))),
            (0b100110, true)  => Ok(Expr::Float(transmute::<_, extern "C" fn(i64, f64, f64, i64, i64, f64) -> f64>(fn_ptr)(iargs[0], fargs[0], fargs[1], iargs[1], iargs[2], fargs[2]))),
            (0b100111, false) => Ok(Expr::Int(transmute::<_, extern "C" fn(f64, f64, f64, i64, i64, f64) -> i64>(fn_ptr)(fargs[0], fargs[1], fargs[2], iargs[0], iargs[1], fargs[3]))),
            (0b100111, true)  => Ok(Expr::Float(transmute::<_, extern "C" fn(f64, f64, f64, i64, i64, f64) -> f64>(fn_ptr)(fargs[0], fargs[1], fargs[2], iargs[0], iargs[1], fargs[3]))),
            (0b101000, false) => Ok(Expr::Int(transmute::<_, extern "C" fn(i64, i64, i64, f64, i64, f64) -> i64>(fn_ptr)(iargs[0], iargs[1], iargs[2], fargs[0], iargs[3], fargs[1]))),
            (0b101000, true)  => Ok(Expr::Float(transmute::<_, extern "C" fn(i64, i64, i64, f64, i64, f64) -> f64>(fn_ptr)(iargs[0], iargs[1], iargs[2], fargs[0], iargs[3], fargs[1]))),
            (0b101001, false) => Ok(Expr::Int(transmute::<_, extern "C" fn(f64, i64, i64, f64, i64, f64) -> i64>(fn_ptr)(fargs[0], iargs[0], iargs[1], fargs[1], iargs[2], fargs[2]))),
            (0b101001, true)  => Ok(Expr::Float(transmute::<_, extern "C" fn(f64, i64, i64, f64, i64, f64) -> f64>(fn_ptr)(fargs[0], iargs[0], iargs[1], fargs[1], iargs[2], fargs[2]))),
            (0b101010, false) => Ok(Expr::Int(transmute::<_, extern "C" fn(i64, f64, i64, f64, i64, f64) -> i64>(fn_ptr)(iargs[0], fargs[0], iargs[1], fargs[1], iargs[2], fargs[2]))),
            (0b101010, true)  => Ok(Expr::Float(transmute::<_, extern "C" fn(i64, f64, i64, f64, i64, f64) -> f64>(fn_ptr)(iargs[0], fargs[0], iargs[1], fargs[1], iargs[2], fargs[2]))),
            (0b101011, false) => Ok(Expr::Int(transmute::<_, extern "C" fn(f64, f64, i64, f64, i64, f64) -> i64>(fn_ptr)(fargs[0], fargs[1], iargs[0], fargs[2], iargs[1], fargs[3]))),
            (0b101011, true)  => Ok(Expr::Float(transmute::<_, extern "C" fn(f64, f64, i64, f64, i64, f64) -> f64>(fn_ptr)(fargs[0], fargs[1], iargs[0], fargs[2], iargs[1], fargs[3]))),
            (0b101100, false) => Ok(Expr::Int(transmute::<_, extern "C" fn(i64, i64, f64, f64, i64, f64) -> i64>(fn_ptr)(iargs[0], iargs[1], fargs[0], fargs[1], iargs[2], fargs[2]))),
            (0b101100, true)  => Ok(Expr::Float(transmute::<_, extern "C" fn(i64, i64, f64, f64, i64, f64) -> f64>(fn_ptr)(iargs[0], iargs[1], fargs[0], fargs[1], iargs[2], fargs[2]))),
            (0b101101, false) => Ok(Expr::Int(transmute::<_, extern "C" fn(f64, i64, f64, f64, i64, f64) -> i64>(fn_ptr)(fargs[0], iargs[0], fargs[1], fargs[2], iargs[1], fargs[3]))),
            (0b101101, true)  => Ok(Expr::Float(transmute::<_, extern "C" fn(f64, i64, f64, f64, i64, f64) -> f64>(fn_ptr)(fargs[0], iargs[0], fargs[1], fargs[2], iargs[1], fargs[3]))),
            (0b101110, false) => Ok(Expr::Int(transmute::<_, extern "C" fn(i64, f64, f64, f64, i64, f64) -> i64>(fn_ptr)(iargs[0], fargs[0], fargs[1], fargs[2], iargs[1], fargs[3]))),
            (0b101110, true)  => Ok(Expr::Float(transmute::<_, extern "C" fn(i64, f64, f64, f64, i64, f64) -> f64>(fn_ptr)(iargs[0], fargs[0], fargs[1], fargs[2], iargs[1], fargs[3]))),
            (0b101111, false) => Ok(Expr::Int(transmute::<_, extern "C" fn(f64, f64, f64, f64, i64, f64) -> i64>(fn_ptr)(fargs[0], fargs[1], fargs[2], fargs[3], iargs[0], fargs[4]))),
            (0b101111, true)  => Ok(Expr::Float(transmute::<_, extern "C" fn(f64, f64, f64, f64, i64, f64) -> f64>(fn_ptr)(fargs[0], fargs[1], fargs[2], fargs[3], iargs[0], fargs[4]))),
            (0b110000, false) => Ok(Expr::Int(transmute::<_, extern "C" fn(i64, i64, i64, i64, f64, f64) -> i64>(fn_ptr)(iargs[0], iargs[1], iargs[2], iargs[3], fargs[0], fargs[1]))),
            (0b110000, true)  => Ok(Expr::Float(transmute::<_, extern "C" fn(i64, i64, i64, i64, f64, f64) -> f64>(fn_ptr)(iargs[0], iargs[1], iargs[2], iargs[3], fargs[0], fargs[1]))),
            (0b110001, false) => Ok(Expr::Int(transmute::<_, extern "C" fn(f64, i64, i64, i64, f64, f64) -> i64>(fn_ptr)(fargs[0], iargs[0], iargs[1], iargs[2], fargs[1], fargs[2]))),
            (0b110001, true)  => Ok(Expr::Float(transmute::<_, extern "C" fn(f64, i64, i64, i64, f64, f64) -> f64>(fn_ptr)(fargs[0], iargs[0], iargs[1], iargs[2], fargs[1], fargs[2]))),
            (0b110010, false) => Ok(Expr::Int(transmute::<_, extern "C" fn(i64, f64, i64, i64, f64, f64) -> i64>(fn_ptr)(iargs[0], fargs[0], iargs[1], iargs[2], fargs[1], fargs[2]))),
            (0b110010, true)  => Ok(Expr::Float(transmute::<_, extern "C" fn(i64, f64, i64, i64, f64, f64) -> f64>(fn_ptr)(iargs[0], fargs[0], iargs[1], iargs[2], fargs[1], fargs[2]))),
            (0b110011, false) => Ok(Expr::Int(transmute::<_, extern "C" fn(f64, f64, i64, i64, f64, f64) -> i64>(fn_ptr)(fargs[0], fargs[1], iargs[0], iargs[1], fargs[2], fargs[3]))),
            (0b110011, true)  => Ok(Expr::Float(transmute::<_, extern "C" fn(f64, f64, i64, i64, f64, f64) -> f64>(fn_ptr)(fargs[0], fargs[1], iargs[0], iargs[1], fargs[2], fargs[3]))),
            (0b110100, false) => Ok(Expr::Int(transmute::<_, extern "C" fn(i64, i64, f64, i64, f64, f64) -> i64>(fn_ptr)(iargs[0], iargs[1], fargs[0], iargs[2], fargs[1], fargs[2]))),
            (0b110100, true)  => Ok(Expr::Float(transmute::<_, extern "C" fn(i64, i64, f64, i64, f64, f64) -> f64>(fn_ptr)(iargs[0], iargs[1], fargs[0], iargs[2], fargs[1], fargs[2]))),
            (0b110101, false) => Ok(Expr::Int(transmute::<_, extern "C" fn(f64, i64, f64, i64, f64, f64) -> i64>(fn_ptr)(fargs[0], iargs[0], fargs[1], iargs[1], fargs[2], fargs[3]))),
            (0b110101, true)  => Ok(Expr::Float(transmute::<_, extern "C" fn(f64, i64, f64, i64, f64, f64) -> f64>(fn_ptr)(fargs[0], iargs[0], fargs[1], iargs[1], fargs[2], fargs[3]))),
            (0b110110, false) => Ok(Expr::Int(transmute::<_, extern "C" fn(i64, f64, f64, i64, f64, f64) -> i64>(fn_ptr)(iargs[0], fargs[0], fargs[1], iargs[1], fargs[2], fargs[3]))),
            (0b110110, true)  => Ok(Expr::Float(transmute::<_, extern "C" fn(i64, f64, f64, i64, f64, f64) -> f64>(fn_ptr)(iargs[0], fargs[0], fargs[1], iargs[1], fargs[2], fargs[3]))),
            (0b110111, false) => Ok(Expr::Int(transmute::<_, extern "C" fn(f64, f64, f64, i64, f64, f64) -> i64>(fn_ptr)(fargs[0], fargs[1], fargs[2], iargs[0], fargs[3], fargs[4]))),
            (0b110111, true)  => Ok(Expr::Float(transmute::<_, extern "C" fn(f64, f64, f64, i64, f64, f64) -> f64>(fn_ptr)(fargs[0], fargs[1], fargs[2], iargs[0], fargs[3], fargs[4]))),
            (0b111000, false) => Ok(Expr::Int(transmute::<_, extern "C" fn(i64, i64, i64, f64, f64, f64) -> i64>(fn_ptr)(iargs[0], iargs[1], iargs[2], fargs[0], fargs[1], fargs[2]))),
            (0b111000, true)  => Ok(Expr::Float(transmute::<_, extern "C" fn(i64, i64, i64, f64, f64, f64) -> f64>(fn_ptr)(iargs[0], iargs[1], iargs[2], fargs[0], fargs[1], fargs[2]))),
            (0b111001, false) => Ok(Expr::Int(transmute::<_, extern "C" fn(f64, i64, i64, f64, f64, f64) -> i64>(fn_ptr)(fargs[0], iargs[0], iargs[1], fargs[1], fargs[2], fargs[3]))),
            (0b111001, true)  => Ok(Expr::Float(transmute::<_, extern "C" fn(f64, i64, i64, f64, f64, f64) -> f64>(fn_ptr)(fargs[0], iargs[0], iargs[1], fargs[1], fargs[2], fargs[3]))),
            (0b111010, false) => Ok(Expr::Int(transmute::<_, extern "C" fn(i64, f64, i64, f64, f64, f64) -> i64>(fn_ptr)(iargs[0], fargs[0], iargs[1], fargs[1], fargs[2], fargs[3]))),
            (0b111010, true)  => Ok(Expr::Float(transmute::<_, extern "C" fn(i64, f64, i64, f64, f64, f64) -> f64>(fn_ptr)(iargs[0], fargs[0], iargs[1], fargs[1], fargs[2], fargs[3]))),
            (0b111011, false) => Ok(Expr::Int(transmute::<_, extern "C" fn(f64, f64, i64, f64, f64, f64) -> i64>(fn_ptr)(fargs[0], fargs[1], iargs[0], fargs[2], fargs[3], fargs[4]))),
            (0b111011, true)  => Ok(Expr::Float(transmute::<_, extern "C" fn(f64, f64, i64, f64, f64, f64) -> f64>(fn_ptr)(fargs[0], fargs[1], iargs[0], fargs[2], fargs[3], fargs[4]))),
            (0b111100, false) => Ok(Expr::Int(transmute::<_, extern "C" fn(i64, i64, f64, f64, f64, f64) -> i64>(fn_ptr)(iargs[0], iargs[1], fargs[0], fargs[1], fargs[2], fargs[3]))),
            (0b111100, true)  => Ok(Expr::Float(transmute::<_, extern "C" fn(i64, i64, f64, f64, f64, f64) -> f64>(fn_ptr)(iargs[0], iargs[1], fargs[0], fargs[1], fargs[2], fargs[3]))),
            (0b111101, false) => Ok(Expr::Int(transmute::<_, extern "C" fn(f64, i64, f64, f64, f64, f64) -> i64>(fn_ptr)(fargs[0], iargs[0], fargs[1], fargs[2], fargs[3], fargs[4]))),
            (0b111101, true)  => Ok(Expr::Float(transmute::<_, extern "C" fn(f64, i64, f64, f64, f64, f64) -> f64>(fn_ptr)(fargs[0], iargs[0], fargs[1], fargs[2], fargs[3], fargs[4]))),
            (0b111110, false) => Ok(Expr::Int(transmute::<_, extern "C" fn(i64, f64, f64, f64, f64, f64) -> i64>(fn_ptr)(iargs[0], fargs[0], fargs[1], fargs[2], fargs[3], fargs[4]))),
            (0b111110, true)  => Ok(Expr::Float(transmute::<_, extern "C" fn(i64, f64, f64, f64, f64, f64) -> f64>(fn_ptr)(iargs[0], fargs[0], fargs[1], fargs[2], fargs[3], fargs[4]))),
            (0b111111, false) => Ok(Expr::Int(transmute::<_, extern "C" fn(f64, f64, f64, f64, f64, f64) -> i64>(fn_ptr)(fargs[0], fargs[1], fargs[2], fargs[3], fargs[4], fargs[5]))),
            (0b111111, true)  => Ok(Expr::Float(transmute::<_, extern "C" fn(f64, f64, f64, f64, f64, f64) -> f64>(fn_ptr)(fargs[0], fargs[1], fargs[2], fargs[3], fargs[4], fargs[5]))),
            _ => unreachable!(),
        }
    }
}


// ── Mixed returning helpers ──────────────────────────────────────────────────

fn call_mixed_1(
    fn_ptr: *const (), pattern: u8,
    iargs: &[i64; 6], fargs: &[f64; 8],
    ret_float: bool,
) -> Result<Expr, String> {
    unsafe {
        match (pattern & 1, ret_float) {
            (0, false) => Ok(Expr::Int(transmute::<_, extern "C" fn(i64) -> i64>(fn_ptr)(iargs[0]))),
            (0, true)  => Ok(Expr::Float(transmute::<_, extern "C" fn(i64) -> f64>(fn_ptr)(iargs[0]))),
            (1, false) => Ok(Expr::Int(transmute::<_, extern "C" fn(f64) -> i64>(fn_ptr)(fargs[0]))),
            (1, true)  => Ok(Expr::Float(transmute::<_, extern "C" fn(f64) -> f64>(fn_ptr)(fargs[0]))),
            _ => unreachable!(),
        }
    }
}

fn call_mixed_2(
    fn_ptr: *const (), pattern: u8,
    iargs: &[i64; 6], fargs: &[f64; 8],
    ret_float: bool,
) -> Result<Expr, String> {
    unsafe {
        match (pattern & 3, ret_float) {
            (0b00, false) => Ok(Expr::Int(transmute::<_, extern "C" fn(i64, i64) -> i64>(fn_ptr)(iargs[0], iargs[1]))),
            (0b00, true)  => Ok(Expr::Float(transmute::<_, extern "C" fn(i64, i64) -> f64>(fn_ptr)(iargs[0], iargs[1]))),
            (0b01, false) => Ok(Expr::Int(transmute::<_, extern "C" fn(f64, i64) -> i64>(fn_ptr)(fargs[0], iargs[0]))),
            (0b01, true)  => Ok(Expr::Float(transmute::<_, extern "C" fn(f64, i64) -> f64>(fn_ptr)(fargs[0], iargs[0]))),
            (0b10, false) => Ok(Expr::Int(transmute::<_, extern "C" fn(i64, f64) -> i64>(fn_ptr)(iargs[0], fargs[0]))),
            (0b10, true)  => Ok(Expr::Float(transmute::<_, extern "C" fn(i64, f64) -> f64>(fn_ptr)(iargs[0], fargs[0]))),
            (0b11, false) => Ok(Expr::Int(transmute::<_, extern "C" fn(f64, f64) -> i64>(fn_ptr)(fargs[0], fargs[1]))),
            (0b11, true)  => Ok(Expr::Float(transmute::<_, extern "C" fn(f64, f64) -> f64>(fn_ptr)(fargs[0], fargs[1]))),
            _ => unreachable!(),
        }
    }
}

fn call_mixed_3(
    fn_ptr: *const (), pattern: u8,
    iargs: &[i64; 6], fargs: &[f64; 8],
    ret_float: bool,
) -> Result<Expr, String> {
    unsafe {
        match (pattern & 7, ret_float) {
            (0b000, false) => Ok(Expr::Int(transmute::<_, extern "C" fn(i64, i64, i64) -> i64>(fn_ptr)(iargs[0], iargs[1], iargs[2]))),
            (0b000, true)  => Ok(Expr::Float(transmute::<_, extern "C" fn(i64, i64, i64) -> f64>(fn_ptr)(iargs[0], iargs[1], iargs[2]))),
            (0b001, false) => Ok(Expr::Int(transmute::<_, extern "C" fn(f64, i64, i64) -> i64>(fn_ptr)(fargs[0], iargs[0], iargs[1]))),
            (0b001, true)  => Ok(Expr::Float(transmute::<_, extern "C" fn(f64, i64, i64) -> f64>(fn_ptr)(fargs[0], iargs[0], iargs[1]))),
            (0b010, false) => Ok(Expr::Int(transmute::<_, extern "C" fn(i64, f64, i64) -> i64>(fn_ptr)(iargs[0], fargs[0], iargs[1]))),
            (0b010, true)  => Ok(Expr::Float(transmute::<_, extern "C" fn(i64, f64, i64) -> f64>(fn_ptr)(iargs[0], fargs[0], iargs[1]))),
            (0b011, false) => Ok(Expr::Int(transmute::<_, extern "C" fn(f64, f64, i64) -> i64>(fn_ptr)(fargs[0], fargs[1], iargs[0]))),
            (0b011, true)  => Ok(Expr::Float(transmute::<_, extern "C" fn(f64, f64, i64) -> f64>(fn_ptr)(fargs[0], fargs[1], iargs[0]))),
            (0b100, false) => Ok(Expr::Int(transmute::<_, extern "C" fn(i64, i64, f64) -> i64>(fn_ptr)(iargs[0], iargs[1], fargs[0]))),
            (0b100, true)  => Ok(Expr::Float(transmute::<_, extern "C" fn(i64, i64, f64) -> f64>(fn_ptr)(iargs[0], iargs[1], fargs[0]))),
            (0b101, false) => Ok(Expr::Int(transmute::<_, extern "C" fn(f64, i64, f64) -> i64>(fn_ptr)(fargs[0], iargs[0], fargs[1]))),
            (0b101, true)  => Ok(Expr::Float(transmute::<_, extern "C" fn(f64, i64, f64) -> f64>(fn_ptr)(fargs[0], iargs[0], fargs[1]))),
            (0b110, false) => Ok(Expr::Int(transmute::<_, extern "C" fn(i64, f64, f64) -> i64>(fn_ptr)(iargs[0], fargs[0], fargs[1]))),
            (0b110, true)  => Ok(Expr::Float(transmute::<_, extern "C" fn(i64, f64, f64) -> f64>(fn_ptr)(iargs[0], fargs[0], fargs[1]))),
            (0b111, false) => Ok(Expr::Int(transmute::<_, extern "C" fn(f64, f64, f64) -> i64>(fn_ptr)(fargs[0], fargs[1], fargs[2]))),
            (0b111, true)  => Ok(Expr::Float(transmute::<_, extern "C" fn(f64, f64, f64) -> f64>(fn_ptr)(fargs[0], fargs[1], fargs[2]))),
            _ => unreachable!(),
        }
    }
}

fn call_mixed_4(
    fn_ptr: *const (), pattern: u8,
    iargs: &[i64; 6], fargs: &[f64; 8],
    ret_float: bool,
) -> Result<Expr, String> {
    unsafe {
        match (pattern & 15, ret_float) {
            (0b0000, false) => Ok(Expr::Int(transmute::<_, extern "C" fn(i64, i64, i64, i64) -> i64>(fn_ptr)(iargs[0], iargs[1], iargs[2], iargs[3]))),
            (0b0000, true)  => Ok(Expr::Float(transmute::<_, extern "C" fn(i64, i64, i64, i64) -> f64>(fn_ptr)(iargs[0], iargs[1], iargs[2], iargs[3]))),
            (0b0001, false) => Ok(Expr::Int(transmute::<_, extern "C" fn(f64, i64, i64, i64) -> i64>(fn_ptr)(fargs[0], iargs[0], iargs[1], iargs[2]))),
            (0b0001, true)  => Ok(Expr::Float(transmute::<_, extern "C" fn(f64, i64, i64, i64) -> f64>(fn_ptr)(fargs[0], iargs[0], iargs[1], iargs[2]))),
            (0b0010, false) => Ok(Expr::Int(transmute::<_, extern "C" fn(i64, f64, i64, i64) -> i64>(fn_ptr)(iargs[0], fargs[0], iargs[1], iargs[2]))),
            (0b0010, true)  => Ok(Expr::Float(transmute::<_, extern "C" fn(i64, f64, i64, i64) -> f64>(fn_ptr)(iargs[0], fargs[0], iargs[1], iargs[2]))),
            (0b0011, false) => Ok(Expr::Int(transmute::<_, extern "C" fn(f64, f64, i64, i64) -> i64>(fn_ptr)(fargs[0], fargs[1], iargs[0], iargs[1]))),
            (0b0011, true)  => Ok(Expr::Float(transmute::<_, extern "C" fn(f64, f64, i64, i64) -> f64>(fn_ptr)(fargs[0], fargs[1], iargs[0], iargs[1]))),
            (0b0100, false) => Ok(Expr::Int(transmute::<_, extern "C" fn(i64, i64, f64, i64) -> i64>(fn_ptr)(iargs[0], iargs[1], fargs[0], iargs[2]))),
            (0b0100, true)  => Ok(Expr::Float(transmute::<_, extern "C" fn(i64, i64, f64, i64) -> f64>(fn_ptr)(iargs[0], iargs[1], fargs[0], iargs[2]))),
            (0b0101, false) => Ok(Expr::Int(transmute::<_, extern "C" fn(f64, i64, f64, i64) -> i64>(fn_ptr)(fargs[0], iargs[0], fargs[1], iargs[1]))),
            (0b0101, true)  => Ok(Expr::Float(transmute::<_, extern "C" fn(f64, i64, f64, i64) -> f64>(fn_ptr)(fargs[0], iargs[0], fargs[1], iargs[1]))),
            (0b0110, false) => Ok(Expr::Int(transmute::<_, extern "C" fn(i64, f64, f64, i64) -> i64>(fn_ptr)(iargs[0], fargs[0], fargs[1], iargs[1]))),
            (0b0110, true)  => Ok(Expr::Float(transmute::<_, extern "C" fn(i64, f64, f64, i64) -> f64>(fn_ptr)(iargs[0], fargs[0], fargs[1], iargs[1]))),
            (0b0111, false) => Ok(Expr::Int(transmute::<_, extern "C" fn(f64, f64, f64, i64) -> i64>(fn_ptr)(fargs[0], fargs[1], fargs[2], iargs[0]))),
            (0b0111, true)  => Ok(Expr::Float(transmute::<_, extern "C" fn(f64, f64, f64, i64) -> f64>(fn_ptr)(fargs[0], fargs[1], fargs[2], iargs[0]))),
            (0b1000, false) => Ok(Expr::Int(transmute::<_, extern "C" fn(i64, i64, i64, f64) -> i64>(fn_ptr)(iargs[0], iargs[1], iargs[2], fargs[0]))),
            (0b1000, true)  => Ok(Expr::Float(transmute::<_, extern "C" fn(i64, i64, i64, f64) -> f64>(fn_ptr)(iargs[0], iargs[1], iargs[2], fargs[0]))),
            (0b1001, false) => Ok(Expr::Int(transmute::<_, extern "C" fn(f64, i64, i64, f64) -> i64>(fn_ptr)(fargs[0], iargs[0], iargs[1], fargs[1]))),
            (0b1001, true)  => Ok(Expr::Float(transmute::<_, extern "C" fn(f64, i64, i64, f64) -> f64>(fn_ptr)(fargs[0], iargs[0], iargs[1], fargs[1]))),
            (0b1010, false) => Ok(Expr::Int(transmute::<_, extern "C" fn(i64, f64, i64, f64) -> i64>(fn_ptr)(iargs[0], fargs[0], iargs[1], fargs[1]))),
            (0b1010, true)  => Ok(Expr::Float(transmute::<_, extern "C" fn(i64, f64, i64, f64) -> f64>(fn_ptr)(iargs[0], fargs[0], iargs[1], fargs[1]))),
            (0b1011, false) => Ok(Expr::Int(transmute::<_, extern "C" fn(f64, f64, i64, f64) -> i64>(fn_ptr)(fargs[0], fargs[1], iargs[0], fargs[2]))),
            (0b1011, true)  => Ok(Expr::Float(transmute::<_, extern "C" fn(f64, f64, i64, f64) -> f64>(fn_ptr)(fargs[0], fargs[1], iargs[0], fargs[2]))),
            (0b1100, false) => Ok(Expr::Int(transmute::<_, extern "C" fn(i64, i64, f64, f64) -> i64>(fn_ptr)(iargs[0], iargs[1], fargs[0], fargs[1]))),
            (0b1100, true)  => Ok(Expr::Float(transmute::<_, extern "C" fn(i64, i64, f64, f64) -> f64>(fn_ptr)(iargs[0], iargs[1], fargs[0], fargs[1]))),
            (0b1101, false) => Ok(Expr::Int(transmute::<_, extern "C" fn(f64, i64, f64, f64) -> i64>(fn_ptr)(fargs[0], iargs[0], fargs[1], fargs[2]))),
            (0b1101, true)  => Ok(Expr::Float(transmute::<_, extern "C" fn(f64, i64, f64, f64) -> f64>(fn_ptr)(fargs[0], iargs[0], fargs[1], fargs[2]))),
            (0b1110, false) => Ok(Expr::Int(transmute::<_, extern "C" fn(i64, f64, f64, f64) -> i64>(fn_ptr)(iargs[0], fargs[0], fargs[1], fargs[2]))),
            (0b1110, true)  => Ok(Expr::Float(transmute::<_, extern "C" fn(i64, f64, f64, f64) -> f64>(fn_ptr)(iargs[0], fargs[0], fargs[1], fargs[2]))),
            (0b1111, false) => Ok(Expr::Int(transmute::<_, extern "C" fn(f64, f64, f64, f64) -> i64>(fn_ptr)(fargs[0], fargs[1], fargs[2], fargs[3]))),
            (0b1111, true)  => Ok(Expr::Float(transmute::<_, extern "C" fn(f64, f64, f64, f64) -> f64>(fn_ptr)(fargs[0], fargs[1], fargs[2], fargs[3]))),
            _ => unreachable!(),
        }
    }
}
