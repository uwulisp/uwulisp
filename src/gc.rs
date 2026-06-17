//! A minimal mark-and-sweep garbage collector for `EnvData` nodes.
//!
//! # How it works
//!
//! Every environment is stored inside a `Heap` as a slot:
//!
//! ```text
//!   Heap.slots: [ Slot { data: EnvData, marked: bool }, ... ]
//! ```
//!
//! A `GcHandle` is just a `usize` index into that `Vec`.  It is `Copy`,
//! so it can live inside `Expr::Lambda` without any reference counting.
//!
//! ## Allocation
//! `Heap::alloc` pushes a new `Slot` and returns its index.
//!
//! ## Mark phase
//! The caller passes a set of *root* handles (the currently-reachable
//! environments — e.g. the global env and every env on the eval call
//! stack).  `Heap::mark` walks each root's `parent` chain recursively,
//! setting `marked = true` on every reachable slot.
//!
//! ## Sweep phase
//! `Heap::sweep` iterates all slots.  Any slot with `marked = false` is
//! freed (replaced with `None`).  All remaining slots are then unmarked,
//! ready for the next cycle.
//!
//! ## Compaction (optional)
//! We do *not* compact by default, so `GcHandle` values remain stable
//! forever.  Free slots are reused by `alloc` (first-fit scan).

use std::collections::HashMap;

use crate::expr::Expr;

// ── public handle ────────────────────────────────────────────────────────────

/// A GC-managed reference to an `EnvData` node.
///
/// `GcHandle` is `Copy` and has no destructor, so it can be embedded
/// anywhere (e.g. `Expr::Lambda`) without creating reference cycles.
/// It becomes dangling after a sweep that collected the slot — callers
/// must ensure they pass all live roots before collecting.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct GcHandle(pub usize);

// ── EnvData ──────────────────────────────────────────────────────────────────

/// The runtime data stored at each environment frame.
pub struct EnvData {
    pub vars:   HashMap<String, Expr>,
    pub parent: Option<GcHandle>,
}

// ── internal slot ────────────────────────────────────────────────────────────

struct Slot {
    data:   EnvData,
    marked: bool,
}

// ── Heap ─────────────────────────────────────────────────────────────────────

/// The GC heap.  There is typically one per interpreter instance.
///
/// Keep it alive for the entire lifetime of the interpreter; all
/// `GcHandle` values are indices into this `Heap`.
pub struct Heap {
    slots: Vec<Option<Slot>>,
}

impl Heap {
    pub fn new() -> Self {
        Heap { slots: Vec::new() }
    }

    // ── allocation ───────────────────────────────────────────────────────────

    /// Allocate a new `EnvData` node and return its handle.
    ///
    /// Reuses the first free (swept) slot if one exists; otherwise appends.
    pub fn alloc(&mut self, parent: Option<GcHandle>) -> GcHandle {
        let data = EnvData { vars: HashMap::new(), parent };
        let slot = Slot { data, marked: false };

        // Try to reuse a free slot.
        for (i, s) in self.slots.iter_mut().enumerate() {
            if s.is_none() {
                *s = Some(slot);
                return GcHandle(i);
            }
        }

        // No free slot — grow the heap.
        let idx = self.slots.len();
        self.slots.push(Some(slot));
        GcHandle(idx)
    }

    // ── slot access ──────────────────────────────────────────────────────────

    /// Borrow the `EnvData` for `handle`.
    ///
    /// # Panics
    /// Panics if the slot has been swept (dangling handle).
    pub fn get(&self, handle: GcHandle) -> &EnvData {
        &self.slot(handle).data
    }

    /// Mutably borrow the `EnvData` for `handle`.
    pub fn get_mut(&mut self, handle: GcHandle) -> &mut EnvData {
        &mut self.slot_mut(handle).data
    }

    // We can't return `&EnvData` and `&mut EnvData` simultaneously, so
    // expose a pair of dedicated helpers instead.

    fn slot(&self, h: GcHandle) -> &Slot {
        self.slots[h.0]
            .as_ref()
            .expect("GcHandle is dangling")
    }

    fn slot_mut(&mut self, h: GcHandle) -> &mut Slot {
        self.slots[h.0]
            .as_mut()
            .expect("GcHandle is dangling")
    }

    /// Read a variable from `handle`'s env, walking the parent chain.
    pub fn env_get(&self, handle: GcHandle, name: &str) -> Result<Expr, String> {
        let data = &self.slot(handle).data;
        if let Some(v) = data.vars.get(name) {
            return Ok(v.clone());
        }
        match data.parent {
            Some(parent) => self.env_get(parent, name),
            None         => Err(format!("undefined symbol: {}", name)),
        }
    }

    /// Bind `name` → `val` in `handle`'s frame (does not walk parents).
    pub fn env_set(&mut self, handle: GcHandle, name: String, val: Expr) {
        self.slot_mut(handle).data.vars.insert(name, val);
    }

    // ── GC cycle ─────────────────────────────────────────────────────────────

    /// **Mark phase.**
    ///
    /// Mark every slot reachable from `roots`, including all ancestors
    /// reachable via `parent` links, and all `GcHandle`s found inside
    /// `Expr::Lambda` values stored in those envs.
    ///
    /// `roots` should contain every `GcHandle` that is directly reachable
    /// from the Rust stack (global env, current call-stack envs, etc.).
    pub fn mark(&mut self, roots: &[GcHandle]) {
        // Use an explicit stack to avoid recursion-depth limits on deep
        // closure chains.
        let mut stack: Vec<GcHandle> = roots.to_vec();

        while let Some(h) = stack.pop() {
            let slot = match self.slots[h.0].as_mut() {
                Some(s) => s,
                None    => continue, // already swept / never allocated
            };

            if slot.marked {
                continue; // already visited
            }
            slot.marked = true;

            // Collect handles we need to visit next (parent + any lambdas).
            // We must collect them first to avoid borrow conflicts.
            let parent  = slot.data.parent;
            let lambdas: Vec<GcHandle> = slot.data.vars.values()
                .filter_map(|expr| {
                    if let Expr::Lambda(_, _, env_h) = expr {
                        Some(*env_h)
                    } else {
                        None
                    }
                })
                .collect();

            if let Some(p) = parent { stack.push(p); }
            stack.extend(lambdas);
        }
    }

    /// **Sweep phase.**
    ///
    /// Free every unmarked slot, then clear all mark bits.
    /// Returns the number of slots that were freed.
    pub fn sweep(&mut self) -> usize {
        let mut freed = 0;
        for slot in self.slots.iter_mut() {
            match slot {
                Some(s) if !s.marked => {
                    *slot = None;
                    freed += 1;
                }
                Some(s) => {
                    s.marked = false; // reset for next cycle
                }
                None => {}
            }
        }
        freed
    }

    /// Convenience: run a full mark-and-sweep cycle.
    ///
    /// Returns the number of freed slots.
    pub fn collect(&mut self, roots: &[GcHandle]) -> usize {
        self.mark(roots);
        self.sweep()
    }

    // ── diagnostics ──────────────────────────────────────────────────────────

    /// Total number of slots ever allocated (including free slots).
    pub fn capacity(&self) -> usize { self.slots.len() }

    /// Number of live (non-freed) slots.
    pub fn live_count(&self) -> usize {
        self.slots.iter().filter(|s| s.is_some()).count()
    }
}

impl Default for Heap {
    fn default() -> Self { Self::new() }
}