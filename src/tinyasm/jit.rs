use std::marker::PhantomData;
use std::ptr;

/// Tracks whether the JIT region is currently writable or executable.
/// Upholds the W^X invariant at the type level by gating APIs on state.
#[cfg(target_arch = "x86_64")]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Protection {
    /// `PROT_READ | PROT_WRITE` — code may be written, not executed.
    ReadWrite,
    /// `PROT_READ | PROT_EXEC` — code may be executed, not written.
    ReadExec,
}

/// A region of executable memory for JIT-compiled machine code.
///
/// Memory is allocated via `mmap` with `RW` permissions (writable, not
/// executable) so code can be written, then switched to `RX` (readable,
/// executable) via [`JitMemory::make_executable`].  This upholds the W^X
/// invariant — a page is never simultaneously writable and executable.
///
/// The `state` field enforces the transition at runtime:
/// - [`write`] is rejected after [`make_executable`] has been called.
/// - [`as_fn`] is rejected before [`make_executable`] has been called.
///
/// # Platform notes
/// This implementation targets **x86-64 Linux/macOS** only.  On x86-64 the
/// hardware maintains coherency between the data cache (D$) and instruction
/// cache (I$), so no explicit cache flush is needed after writing code.
/// On architectures with separate I$ (AArch64, RISC-V) you would need to call
/// `__clear_cache` or equivalent before executing newly written code.
#[cfg(target_arch = "x86_64")]
pub struct JitMemory {
    addr: *mut u8,
    /// Actual allocated size (rounded up to a page boundary).
    size: usize,
    /// Current memory-protection state; enforces W^X at runtime.
    state: Protection,
    /// Number of bytes written so far; used to reject out-of-bounds writes.
    written: usize,
    /// Makes `JitMemory` non-`Sync` on stable Rust.
    ///
    /// `impl !Sync` is nightly-only (issue #68318), so we carry a
    /// `PhantomData<*mut ()>` instead.  Raw pointers are neither `Send` nor
    /// `Sync`, so this field causes the compiler to infer `!Sync` for the
    /// whole struct without affecting runtime layout (zero-sized).
    /// We then explicitly re-assert `Send` via `unsafe impl Send` below.
    _not_sync: PhantomData<*mut ()>,
}

// SAFETY: JitMemory owns a unique mmap region.  No Rust reference aliases it.
// Sending to another thread is only safe because ownership is exclusive.
// `Sync` is not implemented: the `PhantomData<*mut ()>` field makes the
// compiler infer `!Sync`, preventing shared `&JitMemory` references across
// threads (which would allow data races on `state` and the mmap region).
#[cfg(target_arch = "x86_64")]
unsafe impl Send for JitMemory {}

#[cfg(target_arch = "x86_64")]
impl JitMemory {
    /// Allocate at least `min_size` bytes of anonymous, private, read-write
    /// memory, rounded up to the system page size.
    pub fn new(min_size: usize) -> Result<Self, String> {
        if min_size == 0 {
            return Err("JIT region size must be greater than zero".into());
        }

        let page_size = Self::page_size();
        // Round up to the nearest page boundary so mprotect is always valid.
        let size = min_size
            .checked_add(page_size - 1)
            .ok_or("JIT region size overflows")?
            / page_size
            * page_size;

        let addr = unsafe {
            libc::mmap(
                ptr::null_mut(),
                size,
                libc::PROT_READ | libc::PROT_WRITE,
                libc::MAP_PRIVATE | libc::MAP_ANONYMOUS,
                -1,
                0,
            )
        };

        if addr == libc::MAP_FAILED {
            return Err(format!(
                "mmap({} bytes) failed: {}",
                size,
                std::io::Error::last_os_error()
            ));
        }

        Ok(JitMemory {
            addr: addr as *mut u8,
            size,
            state: Protection::ReadWrite,
            written: 0,
            _not_sync: PhantomData,
        })
    }

    /// Copy machine-code bytes into the allocated region at the current write
    /// cursor, advancing it by `code.len()`.
    ///
    /// # Errors
    /// - Returns an error if [`make_executable`] has already been called
    ///   (write-after-execute is prohibited).
    /// - Returns an error if `code` would exceed the allocated region.
    pub fn write(&mut self, code: &[u8]) -> Result<(), String> {
        if self.state != Protection::ReadWrite {
            return Err(
                "cannot write to JIT region after make_executable() has been called".into(),
            );
        }

        let new_written = self
            .written
            .checked_add(code.len())
            .ok_or("write length overflows")?;

        if new_written > self.size {
            return Err(format!(
                "write of {} bytes at offset {} would exceed allocated region ({} bytes)",
                code.len(),
                self.written,
                self.size
            ));
        }

        // SAFETY: Both pointers are valid and non-overlapping:
        // - `self.addr + self.written` is within the mmap region.
        // - `code` is a valid Rust slice.
        unsafe {
            ptr::copy_nonoverlapping(code.as_ptr(), self.addr.add(self.written), code.len());
        }

        self.written = new_written;
        Ok(())
    }

    /// Switch memory protection from `RW` → `RX` (write-xor-execute).
    ///
    /// Must be called after [`write`] and before calling [`as_fn`].
    /// After this call, [`write`] will return an error.
    pub fn make_executable(&mut self) -> Result<(), String> {
        if self.state == Protection::ReadExec {
            // Idempotent — already executable, nothing to do.
            return Ok(());
        }

        let ret = unsafe {
            libc::mprotect(
                self.addr as *mut libc::c_void,
                self.size,
                libc::PROT_READ | libc::PROT_EXEC,
            )
        };

        if ret != 0 {
            return Err(format!(
                "mprotect(RX) failed: {}",
                std::io::Error::last_os_error()
            ));
        }

        self.state = Protection::ReadExec;
        Ok(())
    }

    /// Return a callable function pointer to the start of the JIT region.
    ///
    /// # Errors
    /// Returns an error if [`make_executable`] has not yet been called,
    /// preventing execution of potentially uninitialised or still-writable
    /// memory.
    ///
    /// # Safety
    /// The caller must ensure the region contains valid x86-64 machine code
    /// conforming to the `extern "C" fn() -> u64` ABI (callee-saved registers
    /// preserved, return value in RAX).
    ///
    /// Calling the returned pointer with a mismatched ABI or with code that
    /// corrupts the stack is undefined behaviour.
    pub unsafe fn as_fn(&self) -> Result<extern "C" fn() -> u64, String> {
        if self.state != Protection::ReadExec {
            return Err(
                "cannot obtain function pointer before make_executable() has been called".into(),
            );
        }

        if self.written == 0 {
            return Err("JIT region is empty — no code has been written".into());
        }

        // SAFETY: addr is a non-null, page-aligned pointer to at least
        // `self.written` bytes of RX memory.  The caller is responsible for
        // ABI correctness.
        // SAFETY: addr is a valid RX mapping.
        Ok(unsafe { std::mem::transmute::<*mut u8, extern "C" fn() -> u64>(self.addr) })
    }

    /// Returns the OS page size in bytes.
    fn page_size() -> usize {
        // SAFETY: sysconf is always safe to call with _SC_PAGESIZE.
        let ps = unsafe { libc::sysconf(libc::_SC_PAGESIZE) };
        if ps <= 0 { 4096 } else { ps as usize }
    }
}

#[cfg(target_arch = "x86_64")]
impl Drop for JitMemory {
    fn drop(&mut self) {
        unsafe {
            // Revoke all permissions before unmapping.  This limits the
            // damage if any dangling pointer to this region exists elsewhere:
            // any access will fault immediately rather than silently reading
            // or executing stale code.
            //
            // Errors from mprotect/munmap are intentionally ignored in Drop
            // (we cannot propagate them), but the sequence is best-effort.
            libc::mprotect(self.addr as *mut libc::c_void, self.size, libc::PROT_NONE);
            libc::munmap(self.addr as *mut libc::c_void, self.size);
        }
    }
}
