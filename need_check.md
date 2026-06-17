A few things stand out beyond what's already solid here.

**Stale handle reuse is the biggest one.** When a slot is freed by `sweep` and then `alloc` reuses that index for a brand new `EnvData`, any old `GcHandle` that still happens to hold that index will silently resolve to the *wrong* environment instead of panicking. The "dangling" panic in `get`/`slot` only fires while the slot sits empty (`None`); the moment it's reused, the same numeric handle just quietly points at unrelated data. That's a much nastier failure mode than a panic, since it produces wrong behavior instead of a crash. The standard fix is a generational index: give each slot a small `generation: u32` counter that increments every time it's reused, and have `GcHandle` carry `(index, generation)` instead of a bare `usize`. `get`/`get_mut` then check the generation matches before returning a reference.

```rust
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct GcHandle { idx: usize, gen: u32 }

struct Slot { data: EnvData, marked: bool, gen: u32 }
```

This also lets you make the fields private, which addresses a smaller related issue: `GcHandle(pub usize)` lets anyone construct an arbitrary handle out of thin air (`GcHandle(9999)`), which has no business being possible outside `Heap::alloc`.

**Out-of-bounds handles will panic with an unhelpful message.** `slot`/`slot_mut` and the indexing in `mark` (`self.slots[h.0]`) use direct `Vec` indexing rather than `.get()`/`.get_mut()`. If `h.0` is past the end of `slots` entirely (not just a freed slot within range), that's an "index out of bounds" panic rather than your "GcHandle is dangling" message — confusing to debug, and `mark`'s comment ("already swept / never allocated") implies it handles the never-allocated case when it actually only handles freed-but-in-range. Worth switching to `.get_mut(h.0)` everywhere for consistent, bounds-safe behavior.

**Mark only looks at top-level `Expr::Lambda` values in `vars`.** If `Expr` has any compound variants — lists, pairs, vectors, anything that can *contain* a lambda rather than *be* one — a lambda nested inside one of those won't be discovered, so its closed-over environment can get swept out from under a still-reachable closure. Whether this matters depends on what `Expr` actually looks like (not shown here), but if it has any recursive structure, the mark phase needs to walk into it recursively rather than just filter-mapping the immediate values.

**Mark silently ignores dangling roots**, which is inconsistent with `get`/`get_mut` panicking on the same condition. If a caller accidentally passes a stale root into `collect`, that's exactly the kind of bug you'd want surfaced loudly rather than swallowed — consider at least a `debug_assert!` there.

Smaller things: `alloc`'s first-fit scan is O(n) on every allocation since it walks the whole `Vec` looking for a free slot; a simple free-list (e.g. `Vec<usize>` of freed indices, pushed to in `sweep`, popped from in `alloc`) makes both O(1). `env_get` recurses through the parent chain rather than looping, so very deep environment chains risk a stack overflow — `mark` already avoids this with an explicit stack, and `env_get` could do the same. `live_count` is O(n); a running counter updated in `alloc`/`sweep` is free. And `env_set` is documented as not walking parents, which is right for `define` but if your language needs `set!` semantics (mutate the nearest existing binding in an outer scope), you'll want a second method that does walk up looking for an existing key before falling back to a local insert.