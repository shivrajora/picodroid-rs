# http-head

`no_std` (+ `alloc`) byte-level helpers for the HTTP/1.1 message head: find
the end of the head, read the status line and status code, iterate and match
header lines case-insensitively, detect `Transfer-Encoding: chunked`, and
decode a chunked body incrementally with `ChunkDecoder`.

No I/O, no sockets, no dependencies: the caller owns the buffer and the
transport. picodroid's `picodroid.net.HttpURLConnection` natives
(`crates/picodroid-core/src/net/http_connection.rs`) are the first consumer.

```sh
cargo test -p http-head
```
