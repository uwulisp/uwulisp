use std::ffi::CStr;
use std::ptr;
use crate::gc::GcHandle;
use crate::vm::machine::{VM, VmValue};

/// Passed to every JIT-compiled chunk. The JIT frame gives the
/// native code access to the VM's operand stack and environment.
#[repr(C)]
pub struct JitFrame {
    /// Pointer to the base of the VM operand stack slice for this frame.
    pub stack_ptr: *mut u64,
    /// Number of values currently on the stack (in/out).
    pub stack_len: usize,
    /// Opaque handle to the GC environment (passed through to Rust helpers).
    pub env: GcHandle,
    /// Output: result tag (0 = Number, 1 = Nil, …)
    pub result_tag: u64,
    /// Output: result payload (f64 bits for Number, ptr for String, …)
    pub result_val: u64,
    /// Non-zero on runtime error; points to a static Rust error string.
    pub error: *const u8,

    // -- Rust-only fields below this line (not accessed by JIT code) --
    
    /// Parallel array for tags. JIT code doesn't touch this during numeric hot paths,
    /// but helpers will need it.
    pub tag_ptr: *mut u64,
    
    /// Parallel array for full VmValues (used by helpers that need to reconstruct Objects).
    pub val_ptr: *mut VmValue,
    
    pub capacity: usize,
    
    /// Raw pointer to the VM so helpers can access the Heap.
    pub vm_ptr: *mut std::ffi::c_void,
}

impl JitFrame {
    pub fn new(vm: &mut VM) -> Self {
        // We set up a separate parallel array for JIT execution.
        // For phase 1 we just allocate a fresh stack of size 1024.
        let capacity = 1024;
        let mut stack_vals = Vec::with_capacity(capacity);
        stack_vals.resize(capacity, 0u64);
        let actual_capacity = stack_vals.capacity();
        let mut stack_tags = Vec::with_capacity(actual_capacity);
        stack_tags.resize(actual_capacity, 0u64);
        let mut val_refs = Vec::with_capacity(actual_capacity);
        val_refs.resize(actual_capacity, VmValue::Nil);

        let env = vm.frames.last().map(|f| f.env).expect("JIT needs an env");

        let mut frame = JitFrame {
            stack_ptr: stack_vals.as_mut_ptr(),
            tag_ptr: stack_tags.as_mut_ptr(),
            val_ptr: val_refs.as_mut_ptr(),
            stack_len: 0,
            capacity: actual_capacity,
            env,
            result_tag: 0,
            result_val: 0,
            error: ptr::null(),
            vm_ptr: vm as *mut VM as *mut std::ffi::c_void,
        };

        // Leak the vecs so they stay alive during JIT execution
        std::mem::forget(stack_vals);
        std::mem::forget(stack_tags);
        std::mem::forget(val_refs);

        frame
    }

    pub fn into_vm_value(self) -> Result<VmValue, String> {
        // Recover the leaked vecs
        unsafe {
            let _ = Vec::from_raw_parts(self.stack_ptr, self.capacity, self.capacity);
            let _ = Vec::from_raw_parts(self.tag_ptr, self.capacity, self.capacity);
            let _ = Vec::from_raw_parts(self.val_ptr, self.capacity, self.capacity);
        }

        if !self.error.is_null() {
            let err_str = unsafe { CStr::from_ptr(self.error as *const i8).to_string_lossy().into_owned() };
            return Err(err_str);
        }

        match self.result_tag {
            0 => Ok(VmValue::Number(f64::from_bits(self.result_val))),
            1 => Ok(VmValue::Nil),
            // For strings, lists etc., we might retrieve them from a global or the helper would
            // have placed them somewhere. Phase 1 only returns Number/Nil directly.
            _ => Err("Unsupported JIT return tag".to_string()),
        }
    }

    pub fn push_val(&mut self, val: VmValue) {
        if self.stack_len >= self.capacity {
            self.error = "JIT Stack overflow\0".as_ptr();
            return;
        }
        let idx = self.stack_len;
        unsafe {
            match val {
                VmValue::Number(n) => {
                    *self.tag_ptr.add(idx) = 0; // JitTag::Number
                    *self.stack_ptr.add(idx) = n.to_bits();
                }
                VmValue::Nil => {
                    *self.tag_ptr.add(idx) = 1; // JitTag::Nil
                    *self.stack_ptr.add(idx) = 0;
                }
                other => {
                    *self.tag_ptr.add(idx) = 2; // Object
                    *self.stack_ptr.add(idx) = 0;
                    // Move the VmValue into our parallel array
                    std::ptr::write(self.val_ptr.add(idx), other);
                }
            }
        }
        self.stack_len += 1;
    }

    pub fn pop_val(&mut self) -> Result<VmValue, ()> {
        if self.stack_len == 0 {
            self.error = "JIT Stack underflow\0".as_ptr();
            return Err(());
        }
        self.stack_len -= 1;
        let idx = self.stack_len;
        unsafe {
            let tag = *self.tag_ptr.add(idx);
            let val = *self.stack_ptr.add(idx);
            if tag == 0 {
                Ok(VmValue::Number(f64::from_bits(val)))
            } else if tag == 1 {
                Ok(VmValue::Nil)
            } else {
                Ok(std::ptr::replace(self.val_ptr.add(idx), VmValue::Nil))
            }
        }
    }
}

// -- Helpers called from JIT code --

#[unsafe(no_mangle)]
pub unsafe extern "C" fn jit_helper_load_var(frame_ptr: *mut JitFrame, name_ptr: *const u8, name_len: usize) {
    let frame = unsafe { &mut *frame_ptr };
    let vm = unsafe { &mut *(frame.vm_ptr as *mut VM<'_>) };
    let name = unsafe { std::str::from_utf8_unchecked(std::slice::from_raw_parts(name_ptr, name_len)) };

    match vm.heap_mut().env_get(frame.env, name) {
        Ok(expr) => {
            match crate::vm::machine::expr_to_vm_value(&expr, vm.heap_mut()) {
                Ok(val) => frame.push_val(val),
                Err(_) => frame.error = "JIT LoadVar expr error\0".as_ptr(),
            }
        }
        Err(_) => frame.error = "JIT LoadVar undefined variable\0".as_ptr(),
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn jit_helper_store_var(frame_ptr: *mut JitFrame, name_ptr: *const u8, name_len: usize) {
    let frame = unsafe { &mut *frame_ptr };
    let vm = unsafe { &mut *(frame.vm_ptr as *mut VM<'_>) };
    let name = unsafe { std::str::from_utf8_unchecked(std::slice::from_raw_parts(name_ptr, name_len)) };

    if let Ok(val) = frame.pop_val() {
        if let Ok(expr) = crate::vm::machine::vm_value_to_expr(val, vm.heap_mut()) {
            vm.heap_mut().env_set(frame.env, name.to_string(), expr);
        } else {
            frame.error = "JIT StoreVar vm_value_to_expr failed\0".as_ptr();
        }
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn jit_helper_call(frame_ptr: *mut JitFrame, _n_args: usize) {
    let frame = unsafe { &mut *frame_ptr };
    
    // In a full implementation, we'd sync the JIT stack to the VM stack, call do_call,
    // and run the new frame in the interpreter or JIT.
    // For now, this is a placeholder stub.
    frame.error = "JIT Call not fully implemented\0".as_ptr();
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn jit_helper_make_func(frame_ptr: *mut JitFrame, _code_offset: usize) {
    let frame = unsafe { &mut *frame_ptr };
    frame.error = "JIT MakeFunc not fully implemented\0".as_ptr();
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn jit_helper_tree_eval(frame_ptr: *mut JitFrame, _expr_ptr: *const crate::expr::Expr) {
    let frame = unsafe { &mut *frame_ptr };
    frame.error = "JIT TreeEval not fully implemented\0".as_ptr();
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn jit_helper_push_env(frame_ptr: *mut JitFrame) {
    let frame = unsafe { &mut *frame_ptr };
    let vm = unsafe { &mut *(frame.vm_ptr as *mut VM<'_>) };
    let child = crate::expr::new_env(vm.heap_mut(), Some(frame.env));
    frame.env = child;
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn jit_helper_pop_env(frame_ptr: *mut JitFrame) {
    let frame = unsafe { &mut *frame_ptr };
    let vm = unsafe { &mut *(frame.vm_ptr as *mut VM<'_>) };
    if let Some(parent) = vm.heap_mut().parent_of(frame.env) {
        frame.env = parent;
    } else {
        frame.error = "JIT PopEnv: no parent environment\0".as_ptr();
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn jit_helper_store_self(frame_ptr: *mut JitFrame, _name_ptr: *const u8, _name_len: usize) {
    let frame = unsafe { &mut *frame_ptr };
    frame.error = "JIT StoreSelf not fully implemented\0".as_ptr();
}


