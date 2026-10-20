# pd-protobuf

The byte-at-a-time half of `picodroid.protobuf`, as a leaf crate: micropb's
`PbRead` / `PbWrite` over a Java `byte[]` in `pico-jvm`'s array heap
(`heap_io`), and the one-value cursor operations the `CodedInputStream` /
`CodedOutputStream` natives perform (`read_at`, `write_at`). No JVM
references are held past a call, nothing allocates, and nothing outside
`[pos, limit)` of the array is touched.

`picodroid-core`'s `native_handler/protobuf` is the glue that reads the
stream object's `mBuf` / `mPos` / `mLimit` slots, calls in here, stores the
new position and turns a `DecodeError` into `InvalidProtocolBufferException`.
The split exists so the host can test and fuzz the codec without the LVGL
build behind core (`docs/fuzzing.md`, target `protobuf`).

```sh
cargo test -p pd-protobuf --target "$(rustc -vV | sed -n 's/^host: //p')"
```
