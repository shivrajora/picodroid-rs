# Roadmap: linking by hash, and how far the name strings can go (2026-09)

Status: **roadmap, nothing built.** Successor to
docs/designs/class-link-2026-09.md, which moved class metadata into
pack-time link tables but left every name in the class files. This document
answers three questions — how linking coexists with name shrinking, which
names are still shipped, and how many of them could be removed — and lays
out the stages in the order they pay.

## 1. Where we are

**Shrinking and linking compose.** The shrinker rewrites the class bytes
first (the framework at firmware build, an app at pack time); the link-table
builder then hashes whatever spelling it finds. The runtime's `c::` / `m::` /
`d::` constants come from the same shrink map, so the hash the runtime
computes over `c::picodroid_widget_TextView` is the one the packer stored.
The builder never learns that a name was shrunk — the superclass is stored
as written and `java/lang/Object` is recognised by hash. The shrink maps
remain the append-only ABI they were.

**The names are all still there.** A class in a `.papk` (and in the
firmware's framework section) is a standard class file with its constant
pool intact; the link table sits beside it with offsets and hashes. Only the
per-record class names of the PAPK v1 directory are gone. Measured over the
framework corpus, 245 classes (2026-09-29):

| constant-pool Utf8 bytes | no shrink | shrunk |
|---|---|---|
| class names | 32,937 | 8,012 |
| method and field names | 54,503 | 21,285 |
| descriptors | 72,389 | 29,596 |
| string constants (what the app's code sees) | 8,645 | 9,242 |
| attribute names, signatures, other | 2,959 | 14,608 |
| **all Utf8** | **171,433** (51 % of the class bytes) | **82,743** (28 %) |

(The shrunk column is a debug build's output directory, which keeps
line-number attributes; compare the rows, not the totals.) Shrinking already
took the names from ~160 KB to ~59 KB, and much of the remainder is the
3-byte header each Utf8 entry carries rather than characters.

**Who still reads a name at run time:**

| reader | what it reads | when |
|---|---|---|
| method resolution (`find_method_in`) | method name + descriptor | once per miss, to confirm the one signature-hash match |
| class lookup (`find_class_hashed`) | class name | only where two index entries share a hash, or both segments hit |
| field resolution (`field_slot_declared`, `resolve_static_field`) | field name | every miss — there are no field hashes yet |
| `checkcast` / `instanceof` / catch | class name | decoded from the CP before the (hashed) walk |
| native dispatch (handler API) | class, method, descriptor as `&str` | **every native call** — 3.3 % of a graphics workload's CPU |
| `invokedynamic` / lambdas | descriptors, for argument widening | per lambda call |
| `Class.getName()`, `Enum.name()` / `valueOf(String)`, `toString()`'s `Cls@hash`, uncaught-exception logs, Intent and manifest component names | names as *data* | Java semantics — these are strings the program can observe |
| logs, `native miss` hints, `retrace.sh` | any | debugging |

## 2. What can and cannot go

- **Cannot go:** names the program can observe (the last row but one), and
  string constants. An enum's constants are found by name, `getName()`
  returns one, a manifest names an Activity.
- **Can be replaced by a hash:** everything the *runtime* uses a name for —
  resolution, type checks, native dispatch. The hash is already computed at
  pack time for classes, superclasses, interfaces and methods.
- **Can go from the image, with care:** the Utf8 entries that only the
  runtime's linking used, once nothing reads them. That is the member names
  and most descriptors: ~50 KB of the shrunk framework corpus before the
  cost of the hashes that replace them.

The honest size of the prize: an estimated **30–40 KB of framework flash**
(about 2 % of the RP2350 image) and the same proportion of each app. The
speed prize is separate and is the better reason to start: the native
path's name decode is the last interpreter-side item with a visible share
of the CPU.

## 3. Stages

Each stage stands alone, is gated by a measurement, and leaves the tree
shippable. Stop after any of them.

### N1 — Hash-keyed native dispatch (speed; no format change)

The handler API gains the hashes beside the strings:
`dispatch(class_hash, method_hash, class_name, method_name, ctx)`, with the
modules matching on the `u32`s and confirming with one string compare.
`build_support/names.rs` emits `ch::` / `mh::` constants (the hash of each
loaded spelling) next to `c::` / `m::`, panicking at build time on a
collision inside a namespace. The interpreter takes the hashes from the
site's link-table record instead of decoding the constant pool.

- Needs: `MethodrefDesc` to carry the target's class hash and signature
  hash, or a side region indexed like it (+8 B per Methodref; ~18 KB for
  the framework corpus). That is a link-table layout bump (`LINK_MAGIC`
  version 2), not a PAPK major.
- Gate: `native_us` per call on the claudeusage page-turn spans; the DWT
  profile's name-decode share (3.3 % today) and handler-walk share (1.9 %).
  Flash within +20 KB.
- Note: class-link step 6b measured the *graphics module's* class match at
  0.4 % and dropped it. N1 is a different cut — it removes the decode and
  the per-module string matches for every module at once — but if the
  profile after N1 does not show the 3 % gone, stop here.

### N2 — Field and type-check hashes (speed on misses; small)

(2026-09-30: link-table layout 2 gave `FieldInfo` a third word — the field's
slot among its class's own fields and its kind — and every class its
superclass's index in the section. A field's *name* is still compared as
bytes on a miss; the hash below is still to do. See the second round in
docs/designs/class-link-2026-09.md.)

`FieldInfo` gains a name hash; `Fieldref` and class (tag 7) entries get a
descriptor record like `MethodrefDesc`. Field resolution, `checkcast`,
`instanceof` and catch-type matching stop decoding names on a miss.

- Gate: `resolve_us` on cold spans (first visit of each page); about 7 KB
  of framework flash for the class-entry hashes.

### N3 — Collision-free by construction (the enabler for stripping)

Today every hash match is confirmed by comparing bytes. Removing the bytes
means the hash must be unique wherever it is compared:

- within a class (method signature hashes) — already checked over the
  framework corpus by `signature_hashes_are_unique_within_every_framework_class`;
- within a class set (class names) — already rejected at build and pack time;
- **across** the framework and an app, and between an app class and the
  native handler names — new checks, in the packer, which knows the
  framework's shrink map.

We own the spellings, which makes this tractable: the shrinker
(`tools/class-shrink`) can choose target names whose hashes do not collide,
for framework names (append-only map: a new name is tested against every
existing one) and for an app's own names under `--shrink-app`. A name that
cannot be shrunk (public API reached by reflection-like paths, kept names)
and collides is a pack-time error naming both.

- Gate: the packer rejects a synthetic colliding pair; all 101 examples
  pack; the no-shrink lane still works because N4 is opt-in.

### N4 — Strip the linking-only names (size; PAPK minor bump)

With N1–N3 in place, a pack-time pass drops the Utf8 entries nothing reads
at run time — member names and descriptors not reachable from an
observable-name use — and rewrites the constant pool. Kept: class names
(`getName()`, logs), enum constant names, anything a `String` constant or
an annotation-driven feature (`@Inject`, resources) refers to.

- Only under `--shrink`. The debug / no-shrink flow keeps every name, as
  it keeps line numbers today.
- The shrink map gains the hash of every stripped name, so `retrace.sh`
  turns `m#7c3a91e2` in a log back into `setText`.
- Gate: framework flash −30 KB or better; every `qa_*` app, the sim
  nightly and the HIL fleet green in the shrink lanes; a stripped image's
  uncaught-exception log retraces to the same text as today's.

## 4. Risks

- **Debuggability.** A `native miss` or `NoSuchMethod` log is the first
  thing a developer sees when an SDK call is missing; with N4 it carries a
  hash in shrunk builds. The debug flow must stay fully named, and
  `retrace.sh` must handle hashes before N4 ships.
- **Silent wrong dispatch.** A hash collision without a confirming compare
  calls the wrong method. N3 is the whole defence; it must be a build
  failure, never a runtime check, and it must cover framework × app.
- **ABI.** Hashes of shrunk names become part of the framework ↔ app
  contract. They already are in effect (the app's link table stores the
  hash of the framework's spelling), but N4 makes a framework rename that
  changes a hash unrecoverable for an installed app — the shrink map's
  append-only rule has to extend to "and never re-spell".
- **Third-party apps.** The app-store roadmap installs apps built
  elsewhere; the device must refuse an app whose stripped names it cannot
  link (the framework-map-version check already exists for this purpose).

## 5. Recommendation

Do **N1** for the speed — it is the natural continuation of the class-link
work and the only item left in the dispatch path that a profile can see.
Do **N2** if cold-page spans matter after N1. Treat **N3 + N4** as a size
lever to pull when flash is the constraint: today it is not (617 KB free on
the testbench RP2350, 185 KB on the W boards' debug image after the
2304 KB region, 215 KB on the RP2040), and stripped names cost
debuggability on every log line.
