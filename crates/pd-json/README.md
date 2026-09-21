# pd-json

A `no_std` + `alloc` JSON tree for small heaps.

- `parse::parse` reads a document into a `pool::Pool` and returns the root
  `NodeIdx`; numbers keep `org.json` typing (`Int` when it fits, else `Long`,
  else `Double`), and object entries keep insertion order.
- `pool::Pool` caps live nodes (`MAX_NODES`) and string/key bytes
  (`MAX_PAYLOAD_BYTES`); a put or parse over budget fails with
  `PoolError::Exhausted` and leaves the tree unchanged. Nodes may be shared
  under two parents; a link that would form a cycle is refused.
- `Pool::bind` / `Pool::prune` tie node lifetime to external owners identified
  by a `u16` (picodroid: the Java wrapper's heap slot): after the owner's
  collector runs, `prune` drops dead bindings and sweeps unreachable nodes.
- `serialize` writes compact JSON; both directions cap nesting at `MAX_DEPTH`.

No dependencies, no global state: the caller owns the `Pool` and its locking.
picodroid-core wraps one global pool in a scheduler-atomic section.

```sh
cargo test -p pd-json
```
