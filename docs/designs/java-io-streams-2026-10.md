# `java.io` streams and `Adapter.getView` — T3.3 and T3.4 (2026-10-09)

Two milestones of the [Android-parity roadmap](android-parity-roadmap-2026-08.md), built
together on the `java-io-streams` branch: the `java.io` stream hierarchy (T3.3, "the biggest
*code from the internet just works* enabler") and `Adapter.getView` with `convertView`
re-binding on `ListView` (T3.4).

## T3.3 — the `java.io` stream hierarchy

### What ships

Pure-Java SDK classes under `sdk/java/java/io/`, embedded on every board (no natives, no
`BUILTIN_*` dispatch rows):

| Class | Surface |
|-------|---------|
| `InputStream` (abstract, `Closeable`) | `read()` abstract; `read(byte[])`, `read(byte[],int,int)`, `skip`, `available`, `close`, `mark`/`reset`/`markSupported` with the JDK defaults |
| `OutputStream` (abstract, `Closeable`) | `write(int)` abstract; `write(byte[])`, `write(byte[],int,int)`, `flush`, `close` |
| `ByteArrayInputStream`, `ByteArrayOutputStream` | the JDK surface (`toByteArray`, `size`, `reset`, `writeTo`, `toString`, mark/reset) |
| `Reader` (abstract, `Closeable`) | `read(char[],int,int)` and `close` abstract; `read()`, `read(char[])`, `skip`, `ready`, mark/reset defaults |
| `InputStreamReader` | `(InputStream)`, `(InputStream, String charsetName)`, `getEncoding`, `ready` |
| `BufferedReader` | `(Reader)`, `(Reader, int)`, **`readLine()`**, `read`, `ready`, `close` |
| `Writer` (abstract, `Closeable`) | `write(char[],int,int)`, `flush`, `close` abstract; `write(int)`, `write(char[])`, `write(String)`, `write(String,int,int)`, `append` |
| `OutputStreamWriter` | `(OutputStream)`, `(OutputStream, String)`, `getEncoding` |
| `PrintWriter` | `(Writer)`, `(Writer, boolean autoFlush)`, `(OutputStream)`, `(OutputStream, boolean)`; `print`/`println` for every primitive, `String`, `char[]`, `Object`; `write`, `flush`, `close`, `checkError` — never throws `IOException`, as in the JDK |

Re-parented: `picodroid.io.FileInputStream`/`FileOutputStream` and
`picodroid.net.HttpInputStream`/`HttpOutputStream` now `extend` the `java.io` bases (each gained a
Java `read()` / `write(int)`). New: `Socket.getInputStream()` / `getOutputStream()`, returning
`java.io.InputStream` / `OutputStream` (the JDK descriptors) over `recv` / `send`; the output
stream loops over the 256-byte `send` cap. Name-only classes: `java.io.Closeable` (an interface
edge to `AutoCloseable` in `BUILTIN_INTERFACES`) and `java.io.UnsupportedEncodingException`
(declared by the charset constructors, never thrown — every charset name is accepted).

The idioms this unlocks, verbatim from Android code:

```java
BufferedReader r = new BufferedReader(new InputStreamReader(conn.getInputStream()));
String line;
while ((line = r.readLine()) != null) { ... }

PrintWriter out = new PrintWriter(socket.getOutputStream(), true);
out.println("HELLO");

ByteArrayOutputStream bytes = new ByteArrayOutputStream();
byte[] buf = new byte[64]; int n;
while ((n = in.read(buf)) != -1) bytes.write(buf, 0, n);
```

### Design decisions

- **Chars are bytes.** Strings here are byte-backed (`charAt` and `length` count bytes), so a
  reader hands each byte up as one `char` and a writer sends each `char`'s low byte. There is no
  charset decoding; the charset-name constructors accept any name and report it back. A UTF-8 line
  survives `readLine()` byte for byte, because the line is built with `StringBuilder.append(char)`
  (the raw byte), not `new String(byte[])` (which maps bytes above 0x7F to `?`).
  `ByteArrayOutputStream.toString()` uses `new String(byte[],int,int)` and inherits that mapping,
  as every app that builds a string from bytes already does.
- **No fields on the abstract bases.** Native code addresses `FileInputStream.path`/`pos` and the
  HTTP/socket `handle` by slot (`native_handler/io`, `net/fields.rs`); a base field would shift
  them. The bases are stateless; the byte-array streams keep their state in their own classes.
- **Every concrete stream implements `read()` / `write(int)` in Java.** Native dispatch keys on
  `(class, method name)`, not the descriptor, so an abstract `read()` left unimplemented on
  `FileInputStream` would resolve to the native `read([BII)` arm and misread its arguments.
- **`getInputStream()` returns `java.io.InputStream`.** The JDK descriptor, so a Java method
  that takes a `Socket` compiles against the host JDK and runs here. `HttpURLConnection` keeps
  returning `HttpInputStream` (already compiled into apps' PAPKs); it *is* an `InputStream`, so the
  Android wrapping idiom works unchanged.
- **Socket streams are fresh wrappers per call.** `ServerSocket.accept` allocates a `Socket`
  with the default field count (`objects.alloc`), so a cached-stream field on `Socket` would read
  as `null` there. Wrappers are two-field objects; sharing the socket through them is cheap.
- **The compile-time API contract learnt SDK hierarchy edges.** `verifyApiContract` walks
  `@extends` to resolve an inherited call, but `@extends`/`@implements` rows used to come only from
  `BUILTIN_SUPER`/`BUILTIN_INTERFACES`. A `java/**` class file's superclass and `java/**`
  interfaces are now emitted too (`api_contract.rs::sdk_rows`), so `bufferedReader.read(char[])`
  (owner `BufferedReader`, declared by `Reader`) is admitted. The `java/io/*Reader*` and
  `PrintWriter` "no java.io streams" hints are gone; `FileReader`/`FileWriter` now point at the
  `InputStreamReader`/`OutputStreamWriter` wrapping.
- **`Reader.ready()` made `ready` an SDK member name**, which the `no_original_name_literals`
  guard then found in an unrelated sensor log tag (`sampler.rs`); the tag became `never-ready`.

### Not done

- `java.io.FileInputStream`/`FileOutputStream`/`File` as `java.io` names: the file classes stay in
  `picodroid.io` (`Context.openFileInput` returns `picodroid.io.FileInputStream`). A `java.io.File`
  twin is the obvious next step for ported code and needs its own natives table.
- `FileReader`/`FileWriter`, `DataInputStream`/`DataOutputStream`, `BufferedInputStream`/
  `BufferedOutputStream`, `BufferedWriter`, `StringReader`/`StringWriter`, `java.nio.charset`.
- `Reader implements Readable`, `OutputStream implements Flushable`, `Writer implements
  Appendable`: not declared (each would be another name-only interface nobody has asked for).

## T3.4 — `Adapter.getView` and `convertView` re-binding

### What ships

- `Adapter.getView(int position, View convertView, ViewGroup parent)`, abstract as on Android;
  `BaseAdapter` subclasses implement it.
- `ArrayAdapter.getView` builds a `TextView` (or inflates Android's layout-resource constructors:
  `(Context, int resource[, int textViewResourceId][, T[] | List<T>])`) and re-binds the
  `convertView` it is handed. Also `addAll`, `insert`, `remove`, `getPosition`, `getContext`.
- `ListView` asks `getView` for every position, keeps the rows as its direct children in position
  order (`getChildAt(i)` is row *i*), and on `notifyDataSetChanged()` offers each existing row back
  as `convertView`. A row the adapter replaces is freed; positions past the new count are freed;
  `setAdapter` starts from fresh rows (Android clears its recycler on a new adapter).
  `onItemClick` now receives the row `View`.
- `ListView.nativeStyleRow` (LVGL): a row view is stretched to the list's width, padded, given the
  list-button separator, made clickable and keypad-focusable, joined to the Activity's focus group
  and highlighted when focused — what `lv_list_add_button` did for the text rows, on any view.
- `ViewGroup.addView(View, int index)` / `addView(View, int, LayoutParams)` (Android API, from
  the roadmap's child-list backlog), backed by `lv_obj_move_to_index`.
- The row click callback resolves the *current* target, so a layout row whose child bubbles the
  click still maps to its position.

### Where it diverges from the roadmap line

The roadmap said "pooled to the ~12-row cap". The recycle pool here is **the row set itself**:
rows never scroll out of existence (the list is a plain LVGL scroller, every row is live), so there
is no off-screen scrap heap and nothing to cap. `convertView` re-binding happens on every refresh,
which is the path apps take (`notifyDataSetChanged` after a data change), and the memory cost is
one row view per item — the same as the native text rows cost before, now in Java-visible
widgets. A list long enough to need virtualization (rows created only for the visible window, with
`lv_obj` padding standing in for the rest) is a follow-up; with keypad navigation, the focus ring
and the scroll position would have to be re-targeted as rows are re-bound, which is the hard part.

The `nativeBindAdapter` upcall loop (the T2.5 proof consumer) is gone: the whole loop is Java now,
with no native→Java upcalls per row. `invoke_java` keeps its other consumers.

### Verified

- `qa_ui` (`adapters` section): a two-line `LinearLayout` row adapter in the Android shape —
  built on `null`, re-bound in place with identity kept, a new adapter from fresh rows, shrink and
  growth, `ArrayAdapter` rows as `TextView`s, `insert`/`remove`/`getPosition`.
- `qa_io` (new, every board): byte-array streams, the base-class defaults an app's own stream
  inherits, `readLine` across `\n`, `\r\n`, `\r`, a line longer than the buffer, UTF-8 bytes kept,
  writers, the file streams through their `java.io` supertypes, try-with-resources, `instanceof`.
- `netexception` (`stream-loopback`, sim on the W board): `PrintWriter` over
  `getOutputStream()` into `BufferedReader` over `getInputStream()`, a 700-char line over the
  256-byte `send` cap, the peer's close as `null`.
- picoenvmon, tutorial_service and keynav (the in-tree `ArrayAdapter` users) build unchanged.

### Flash cost

Measured with `parity-bench.sh --size-only` and accepted into `bench/parity/ratchet.toml`:

| Board | flash before | flash after | delta |
|-------|--------------|-------------|-------|
| testbench_rp2040 | 1,042,920 | 1,066,504 | +23,584 B (112 KB free) |
| testbench_rp2350 | 1,572,936 | 1,598,480 | +25,544 B |

The ten `java/io` class files are ~12.8 KB stripped (`PrintWriter` 3.0 KB, `BufferedReader`
1.9 KB); the rest is `ArrayAdapter`'s constructors and `getView`, the `ListView` rebind loop, the
socket wrappers, and the row-styling and child-move natives. RAM is unchanged. On review the
reader/writer layer (`Reader`, `Writer`, `InputStreamReader`, `BufferedReader`,
`OutputStreamWriter`, `PrintWriter`) went into `testbench_rp2040`'s `framework_class_excludes`:
that board keeps `InputStream`/`OutputStream` (the file streams extend them) and the
`ByteArray*` streams, and `verifyApiContract --board testbench_rp2040` rejects an app that wraps
a reader there. `qa_io` is therefore gated to 2350-class boards in `hil-tests.conf`. The
re-measured cost is in the table below.

| Board | flash before | flash after | delta |
|-------|--------------|-------------|-------|
| testbench_rp2040 | 1,042,920 | 1,053,496 | +10,576 B (125 KB free) |
| testbench_rp2350 | 1,572,936 | 1,598,480 | +25,544 B |
