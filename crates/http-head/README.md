# http-head

`no_std` (+ `alloc`) byte-level helpers for the HTTP/1.1 message head: find
the end of the head, read the status line and status code, iterate and match
header lines case-insensitively, detect `Transfer-Encoding: chunked`, and
decode a chunked body incrementally with `ChunkDecoder`.

No I/O, no sockets, no dependencies: the caller owns the buffer and the
transport. picodroid's `picodroid.net.HttpURLConnection` natives
(`crates/picodroid-core/src/net/http_connection.rs`) are the first consumer.

`conn::RxBuffer<N, CHUNK>` is the receive side of one connection on top of
those helpers: it reads the head as bytes arrive (through a `recv` closure
the caller supplies), keeps it for the header accessors, and hands the body
out — stashed over-read first, then straight from the socket, through
`ChunkDecoder` on a chunked response. `N` and `CHUNK` are const parameters,
so the device's shape (`RX_BUF_SIZE`, `IO_CHUNK`) and a small one for tests
and the `http_conn` fuzz target compile to the same code.

```sh
cargo test -p http-head
```
