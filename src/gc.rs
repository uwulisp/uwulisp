//! A minimal mark-and-sweep garbage collector for `EnvData` nodes.
//!
//! # How it works
//!
//! Every environment is stored inside a `Heap` as a pair of parallel
//! arrays, indexed by slot:
//!
//! ```text
//!   Heap.data: [ Some(EnvData), None,        Some(EnvData), ... ]
//!   Heap.meta: [ {generation: 0, ..},  {generation: 3, ..}, {generation: 1, ..},  ... ]
//! ```
//!
//! `data[i]` is `None` exactly when slot `i` is free. `meta[i].generation` is a
//! counter that increments every time slot `i` is *reused* by `alloc`,
//! independent of whether `data[i]` currently holds anything — this is
//! the key piece that lets us detect stale handles (see below).
//!
//! A `GcHandle` is `{ idx, generation }`: an index into those arrays plus the
//! generation it was issued at. It is `Copy` and has no destructor, so
//! it can live inside `Expr::Lambda` without any reference counting.
//! Both fields are private — the only way to obtain a `GcHandle` is via
//! `Heap::alloc`, so code outside this module can't conjure up an
//! arbitrary/out-of-thin-air handle.
//!
//! ## Allocation
//! `Heap::alloc` pops an index off a free-list if one exists (O(1)),
//! bumping that slot's generation; otherwise it grows the arrays.
//!
//! ## Stale handles vs. dangling handles
//! Two distinct failure modes are both detected, with distinct-ish
//! messages:
//! * **Freed, not yet reused** (`data[i] == None`) — same as before,
//!   "the slot was swept."
//! * **Reused by something else** (`data[i] == Some(..)` but
//!   `meta[i].generation != handle.generation`) — this is the dangerous case a plain
//!   `usize` handle can't distinguish: the index is alive, but it's
//!   *not the same environment* the handle was originally issued for.
//!   Without the generation counter this would silently return the
//!   wrong `EnvData` instead of erroring.
//!
//! Out-of-range indices (handle never allocated, or heap was somehow
//! given a handle from a different `Heap`) are also handled explicitly
//! via `.get()`, so they produce the same friendly panic message
//! instead of a raw "index out of bounds."
//!
//! ## Mark phase
//! The caller passes a set of *root* handles (the currently-reachable
//! environments — e.g. the global env and every env on the eval call
//! stack). `Heap::mark` walks each root's `parent` chain, and also walks
//! *into* every `Expr` stored in `vars` looking for `Expr::Lambda`
//! values — including ones nested inside `Expr::List`, the bodies of
//! other lambdas, and `Expr::Macro` bodies — not just top-level values.
//! Each lambda found contributes its captured-env `GcHandle` as
//! something to mark.
//!
//! The *heap-graph* walk (parent links, lambda-to-lambda chains) uses an
//! explicit work stack rather than recursion, since that graph can be
//! arbitrarily deep/cyclic. The *Expr-tree* walk inside a single value
//! is plain recursion, since an AST's nesting depth is bounded by how
//! deeply the source program nests expressions — a much smaller and
//! more predictable bound than heap depth.
//!
//! A handle that turns out to be out-of-range, freed, or stale while
//! walking is *not* a normal occurrence — `roots` should only ever
//! contain handles that are still genuinely alive — so hitting one is
//! treated as a bug and flagged via `debug_assert!` (loud in debug
//! builds, but the release build stays lenient and just skips it rather
//! than crashing the collector itself).
//!
//! ## Sweep phase
//! `Heap::sweep` iterates all slots. Any slot with `marked == false` is
//! freed (`data[i]` set to `None`, index pushed onto the free-list).
//! All remaining slots are then unmarked, ready for the next cycle.
//!
//! ## Compaction (optional)
//! We do *not* compact by default, so the `idx` half of a `GcHandle`
//! remains stable forever. Freed slots are reused by `alloc` via the
//! free-list (O(1) instead of the old O(n) first-fit scan).

use std::collections::HashMap;

use crate::expr::Expr;

// ── public handle ────────────────────────────────────────────────────────────

/// A GC-managed reference to an `EnvData` node.
///
/// `GcHandle` is `Copy` and has no destructor, so it can be embedded
/// anywhere (e.g. `Expr::Lambda`) without creating reference cycles.
///
/// Both fields are private. `idx` is the slot index; `generation` is the
/// generation the slot was at when this handle was issued. A handle
/// whose `generation` no longer matches the slot's current generation is
/// stale — the slot has been freed and reused for an unrelated
/// `EnvData` since this handle was created — and `Heap` will refuse to
/// resolve it rather than silently aliasing the wrong environment.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct GcHandle {
    idx: usize,
    generation: u32,
}

// ── EnvData ──────────────────────────────────────────────────────────────────

/// The runtime data stored at each environment frame.
pub struct EnvData {
    pub vars:   HashMap<String, Expr>,
    pub parent: Option<GcHandle>,
}

// ── internal slot metadata ──────────────────────────────────────────────────

/// Per-slot bookkeeping that must outlive the slot's *contents* being
/// freed — in particular `generation`, which has to keep counting even while
/// `data[idx]` is `None`, or a freed-then-reused slot would reset back
/// to a generation an old handle could match again.
struct SlotMeta {
    marked: bool,
    generation: u32,
}

// ── Heap ─────────────────────────────────────────────────────────────────────

/// The GC heap. There is typically one per interpreter instance.
///
/// Keep it alive for the entire lifetime of the interpreter; all
/// `GcHandle` values are indices into this `Heap`.
pub struct Heap {
    data: Vec<Option<EnvData>>,
    meta: Vec<SlotMeta>,
    /// Indices of currently-free slots, so `alloc` can reuse one in
    /// O(1) instead of scanning `data` for the first `None`.
    free_list: Vec<usize>,
    /// Maintained incrementally by `alloc`/`sweep` so `live_count()` is
    /// O(1) instead of re-scanning every slot.
    live_count: usize,
}

impl Heap {
    pub fn new() -> Self {
        Heap {
            data: Vec::new(),
            meta: Vec::new(),
            free_list: Vec::new(),
            live_count: 0,
        }
    }

    // ── allocation ───────────────────────────────────────────────────────────

    /// Allocate a new `EnvData` node and return its handle.
    ///
    /// Reuses a free slot off the free-list if one exists (O(1));
    /// otherwise grows the heap. Reusing a slot bumps its generation,
    /// which is what invalidates any old `GcHandle` still pointing at
    /// that index.
    pub fn alloc(&mut self, parent: Option<GcHandle>) -> GcHandle {
        let env_data = EnvData { vars: HashMap::new(), parent };
        self.live_count += 1;

        if let Some(idx) = self.free_list.pop() {
            let new_generation = self.meta[idx].generation.wrapping_add(1);
            self.meta[idx] = SlotMeta { marked: false, generation: new_generation };
            self.data[idx] = Some(env_data);
            return GcHandle { idx, generation: new_generation };
        }

        let idx = self.data.len();
        self.data.push(Some(env_data));
        self.meta.push(SlotMeta { marked: false, generation: 0 });
        GcHandle { idx, generation: 0 }
    }

    // ── slot access ──────────────────────────────────────────────────────────

    /// Borrow the `EnvData` for `handle`.
    ///
    /// # Panics
    /// Panics with a descriptive message if the handle is out of
    /// range, points at a freed slot, or is stale (the slot has since
    /// been reused for a different `EnvData`).
    pub fn get(&self, handle: GcHandle) -> &EnvData {
        self.check(handle).unwrap_or_else(|msg| panic!("{}", msg));
        self.data[handle.idx].as_ref().unwrap()
    }

    /// Mutably borrow the `EnvData` for `handle`. Same panic conditions
    /// as `get`.
    pub fn get_mut(&mut self, handle: GcHandle) -> &mut EnvData {
        self.check(handle).unwrap_or_else(|msg| panic!("{}", msg));
        self.data[handle.idx].as_mut().unwrap()
    }

    /// Validate a handle without panicking, returning a descriptive
    /// error otherwise. Uses `.get()` rather than direct indexing so an
    /// out-of-range `idx` produces this message instead of a raw
    /// "index out of bounds" panic.
    fn check(&self, handle: GcHandle) -> Result<(), String> {
        let meta = match self.meta.get(handle.idx) {
            Some(m) => m,
            None => {
                return Err(format!(
                    "GcHandle is dangling: index {} is out of range (heap capacity {})",
                    handle.idx,
                    self.data.len()
                ));
            }
        };
        if meta.generation != handle.generation {
            return Err(format!(
                "GcHandle is dangling: slot {} was freed and reused (handle generation {}, slot generation {})",
                handle.idx, handle.generation, meta.generation
            ));
        }
        if self.data[handle.idx].is_none() {
            return Err(format!("GcHandle is dangling: slot {} was freed", handle.idx));
        }
        Ok(())
    }

    /// Read a variable from `handle`'s env, walking the parent chain.
    ///
    /// Iterative rather than recursive, so a very deep environment
    /// chain (e.g. many nested `let`s) can't blow the Rust call stack —
    /// the same reasoning `mark` already applies to the heap graph.
    pub fn env_get(&self, handle: GcHandle, name: &str) -> Result<Expr, String> {
        let mut current = handle;
        loop {
            let data = self.get(current);
            if let Some(v) = data.vars.get(name) {
                return Ok(v.clone());
            }
            match data.parent {
                Some(parent) => current = parent,
                None => return Err(format!("undefined symbol: {}", name)),
            }
        }
    }

    /// Bind `name` → `val` in `handle`'s frame (does not walk parents).
    /// This is `define` semantics: always creates/overwrites a binding
    /// in the innermost frame.
    pub fn env_set(&mut self, handle: GcHandle, name: String, val: Expr) {
        self.get_mut(handle).vars.insert(name, val);
    }

    /// `set!` semantics: walk outward from `handle` looking for an
    /// *existing* binding of `name` and mutate it in place wherever
    /// it's found. Unlike `env_set`, this does not create a new binding
    /// in the local frame — if no existing binding is found anywhere up
    /// the parent chain, it returns an error instead of silently
    /// falling back to a local `define`.
    pub fn env_assign(&mut self, handle: GcHandle, name: &str, val: Expr) -> Result<(), String> {
        let mut current = handle;
        loop {
            if self.get(current).vars.contains_key(name) {
                self.get_mut(current).vars.insert(name.to_string(), val);
                return Ok(());
            }
            let parent = self.get(current).parent;
            match parent {
                Some(p) => current = p,
                None => return Err(format!("set!: undefined symbol: {}", name)),
            }
        }
    }

    // ── GC cycle ─────────────────────────────────────────────────────────────

    /// **Mark phase.**
    ///
    /// Mark every slot reachable from `roots`: all ancestors reachable
    /// via `parent` links, plus every `GcHandle` found inside any
    /// `Expr::Lambda` reachable from a slot's `vars` — including
    /// lambdas nested inside `Expr::List`, lambda bodies, and macro
    /// bodies, not just top-level values.
    ///
    /// `roots` should contain every `GcHandle` that is directly
    /// reachable from the Rust stack (global env, current call-stack
    /// envs, etc.). A root that's out of range, freed, or stale
    /// indicates a bug in the caller (it should never have been
    /// reachable in the first place); this is flagged via
    /// `debug_assert!` rather than either panicking unconditionally or
    /// silently ignoring it, since dropping a still-needed root
    /// silently would otherwise manifest later as a confusing "use
    /// after collection" panic far from the actual bug.
    pub fn mark(&mut self, roots: &[GcHandle]) {
        let mut stack: Vec<GcHandle> = roots.to_vec();

        while let Some(h) = stack.pop() {
            // Validate the handle and pull out plain (Copy) values right
            // away rather than holding a `&SlotMeta`/`&EnvData` across
            // the branches below — keeps the borrows trivially short
            // and side-steps any question of exactly when they end.
            let (slot_generation, already_marked) = match self.meta.get(h.idx) {
                Some(m) => (m.generation, m.marked),
                None => {
                    debug_assert!(false, "mark: handle {:?} has an out-of-range index", h);
                    continue;
                }
            };
            if slot_generation != h.generation {
                debug_assert!(false, "mark: handle {:?} is stale (slot generation is {})", h, slot_generation);
                continue;
            }
            if already_marked {
                continue; // already visited
            }

            let (parent, found) = match self.data[h.idx].as_ref() {
                Some(slot_data) => {
                    let parent = slot_data.parent;
                    let mut found: Vec<GcHandle> = Vec::new();
                    for expr in slot_data.vars.values() {
                        collect_lambda_envs(expr, &mut found);
                    }
                    (parent, found)
                }
                None => {
                    debug_assert!(false, "mark: handle {:?} points at a freed slot", h);
                    continue;
                }
            };

            self.meta[h.idx].marked = true;

            if let Some(p) = parent { stack.push(p); }
            stack.extend(found);
        }
    }

    /// **Sweep phase.**
    ///
    /// Free every unmarked slot (pushing it onto the free-list for
    /// reuse), then clear all mark bits.
    /// Returns the number of slots that were freed.
    pub fn sweep(&mut self) -> usize {
        let mut freed = 0;
        for idx in 0..self.data.len() {
            if self.data[idx].is_some() {
                if self.meta[idx].marked {
                    self.meta[idx].marked = false; // reset for next cycle
                } else {
                    self.data[idx] = None;
                    self.free_list.push(idx);
                    self.live_count -= 1;
                    freed += 1;
                }
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
    pub fn capacity(&self) -> usize { self.data.len() }

    /// Number of live (non-freed) slots. O(1) — tracked incrementally
    /// rather than rescanned on every call.
    pub fn live_count(&self) -> usize { self.live_count }
}

impl Default for Heap {
    fn default() -> Self { Self::new() }
}

// ── Expr traversal for marking ──────────────────────────────────────────────

/// Walk `expr`'s tree looking for every `Expr::Lambda`, appending each
/// one's captured-env handle to `out`. This descends into the compound
/// variants that can *contain* a lambda (lists, lambda/macro bodies) so
/// that e.g. a lambda buried inside `(list (lambda (x) x) 1 2)` is still
/// discovered, not just a lambda that's a direct top-level value.
///
/// Recursion here follows the *Expr*'s own nesting (i.e. how deeply the
/// source program nests sub-expressions), which is a much shallower and
/// more predictable bound than the heap's parent-chain depth — that's
/// why `mark`'s outer heap-graph walk uses an explicit stack but this
/// inner walk doesn't need to.
///
/// `Expr::Func` is opaque (an `Rc<dyn Fn>`) and can't be introspected
/// from here; built-ins are assumed not to close over GC-managed envs
/// directly. `Expr::CubicalTerm` is likewise treated as opaque — if
/// cubical terms ever gain the ability to embed `Expr`/`GcHandle`
/// values, this match will need a corresponding arm.
fn collect_lambda_envs(expr: &Expr, out: &mut Vec<GcHandle>) {
    match expr {
        Expr::Lambda(_, body, env_handle) => {
            out.push(*env_handle);
            collect_lambda_envs(body, out);
        }
        Expr::Macro(_, body) => {
            collect_lambda_envs(body, out);
        }
        Expr::List(items) => {
            for item in items {
                collect_lambda_envs(item, out);
            }
        }
        Expr::Symbol(_)
        | Expr::Number(_)
        | Expr::Str(_)
        | Expr::Func(_)
        | Expr::CubicalTerm(_) => {}
    }
}