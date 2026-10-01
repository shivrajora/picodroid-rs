# Pack-time class linking and faster method dispatch (2026-09)

Status: **complete in the working tree, uncommitted** (2026-09-28/29).
Steps 0–5 and 6a landed: negative caching, `crates/class-link`, PAPK v2
with the framework as a class section, `Parsed` gone, hash-based resolution
over a sorted class index, the descriptor-first invoke, the native claim
hint. 6b was dropped on the CPU profile below. Every step was gated by
`scripts/test.sh` (both shrink lanes), the helloworld sim smoke and
`scripts/pre-commit`, and measured on the bench.

**Outcome, testbench_rp2350, release + `--shrink --shrink-app`:** the
benchmark's TOTAL fell 21.6 % (123,619 → 96,955 ms), a virtual call 28 %,
an interface call 25 %, a call on a class-file-less builtin 66 %,
`object_allocation` 40 %, `string_operations` 48 %; the deterministic
parity build's wall −13.8 % (shrink) and −18.7 % (no-shrink) with the
interpreter executing the same instruction stream. Memory on the
claudeusage History page: the JVM arena in use −81 KB (266,200 → 185,112 B)
and the device's free heap +52 KB (115,824 → 168,192 B), the parsed class
metadata gone and the class table 4.5 KB. Cost: +125 KB of RP2350 flash
(617 KB still free), +59 KB on the RP2040 (whose app region moved from 1024
to 768 KB, leaving 215 KB free), papks +19…30 %. Results are filled in as each step lands.

## Why

Every Java invoke resolved by *name*. `op_invoke` decoded the constant-pool
`Methodref` chain (about ten XIP flash reads), validated UTF-8 three times and
walked the descriptor (`count_args`) *before* probing the resolve cache, so a
cache hit still paid all of that. On a miss, `find_class` was an FNV hash plus
a linear scan over ~250 loaded classes (~8 µs on the RP2350) and `find_method`
byte-compared names read from flash. Failed resolutions were never cached, so
every call on a class-file-less builtin (`String`, `StringBuilder`, `Integer`,
`ArrayList`, `HashMap`, the `Object` fall-through) paid three or four
`find_class` scans *per call*. The native dispatch memo remembered only which
module claimed a site; the module then ran 40–90 string compares (graphics).
And the per-class parsed metadata (`Parsed`) was the largest heap consumer:
~76 KB on the RP2350 for claudeusage's History page.

We own the `.papk` format and the framework-class embedding, and nothing is
deployed, so the class files can carry **link tables** — the whole parsed
record plus a signature hash per method, a hash per superclass and interface
name, and a 4-byte descriptor per `Methodref` — and a **sorted class index**,
built once at pack time (apps) and firmware-build time (framework) and read in
place from flash. Combined with negative caching and a descriptor-first hit
path, resolution stops touching constant-pool UTF-8 on the hot path,
`find_class` becomes a binary search, and the parsed-metadata RAM goes away.

Decisions: the papk format change is required (major bump, no legacy path —
nothing is deployed); RP2350 UI apps are the primary target (RAM-neutral or
better; flash has room), RP2040 must still build and fit; framework classes
get exactly the same tables, index, reader and dispatch optimisations as app
classes, by construction (one container format, one loader); benchmarks are
measured in the real-world configuration, `--release --shrink --shrink-app`.

## The link table (`crates/class-link`)

One table per class, little-endian `u16` words, 4-byte aligned, stored right
after its class bytes. Every byte offset is into the class bytes, every word
offset into the table itself. The layout, from `class_link::layout`:

```text
header, 24 words:
 w0  LINK_MAGIC (0x4B02: 'K', layout version 2)
 w1  total_words       the whole table, header included (even)
 w2  class_len         == the class bytes' length
 w3  cp_count          constant_pool_count
 w4  fields_len        instance fields   w5 statics_len   w6 ifaces_len
 w7  methods_len       w8 mrefs_len     (Methodref + InterfaceMethodref)
 w9  methods_off       word offset of the method records
 w10 mrefs_off         word offset of the Methodref descriptors
 w11 bsm_off           byte offset of the BootstrapMethods body, 0 = none
 w12 access_flags
 w13 name_off          byte offset of this class's name Utf8 data ([u16 len][bytes])
 w14 super_off         the same for the superclass name; 0 = no superclass
 w15 source_file_idx   CP index of the SourceFile Utf8, 0 = none
 w16-17 name_hash      u32, low word first
 w18-19 super_hash     u32, 0 when super_off == 0
 w20 strs_len          String entries
 w21 own_slots         slots this class's own instance fields take
 w22 super_idx         the superclass's position in the section, 0xFFFF = not in it
 w23 0                 reserved
regions, in this order:
 cp_words[cp_count]        entry i's data offset (after its tag byte); for a
                           Methodref or a String, the WORD offset of its descriptor
 tags[(cp_count+1)/2]      the tags, two a word, entry i at byte i
 fields[fields_len]        FieldInfo    { name_index, descriptor_index,
                                          slot (12 bits) | kind (4 bits) }      3 words
 statics[statics_len]      FieldInfo                                            3 words
 ifaces[ifaces_len]        IfaceInfo    { utf8_off, hash }                      3 words
 methods[methods_len]      MethodInfo   { info_off, code_off, access_flags,
                                          lnt_off, sig_hash }                   6 words
 mrefs[mrefs_len]          MethodrefDesc{ cp_off, argc, flags }                 2 words
 strs[strs_len]            StringDesc   { cp_off, literal id }                  2 words
 [pad word]                to an even total
```

Layout version 2 (2026-09-30, "Second round" below) added the last four
header words, the third `FieldInfo` word and the `strs` region. Two kinds of
word name something in the *section* rather than in the class — a
`StringDesc`'s literal id and `super_idx` — so a class alone does not
determine them: the section builder fills them in, `Link::validate` skips
them, and `ClassSection::validate` checks each against the section.

What is *not* stored is read from the class bytes at a fixed distance from
something that is: a method's `max_stack`, `max_locals` and `code_length` sit
8, 6 and 4 bytes before its `code_off`; its name and descriptor indices 2 and
4 bytes after its `info_off`. The superclass is stored as written —
`java/lang/Object` included — and the runtime recognises Object by its hash,
so the builder needs no knowledge of the shrunk spelling.

Hashes are 32-bit FNV-1a (`class_link::name_hash`, the same function
`pico_jvm::class_file::name_hash` always computed), over the names as the
class file spells them; under `--shrink` the files are already renamed when
they are linked, so the hashes match what the runtime computes over its own
shrunk constants. `sig_hash(name, desc)` is the FNV-1a of `name ++ desc`.

The builder and the validator run the *same* derivation of the table from the
class bytes (`class_link::classfile::derive`): the builder writes each word,
the validator compares each word after cross-checking the constant-pool words
against an independent walk of the pool. A table that validates is exactly
what the builder would have produced. The builder also refuses what the
runtime could not rely on: a `Methodref` whose parts are the wrong kind, an
unparseable descriptor, a member whose name is not `Utf8`, and any class,
member or descriptor name that is not valid UTF-8 (the runtime reads those
without re-checking).

## The class section

The payload of a PAPK `CLSS` section and, byte for byte, the framework corpus
embedded in firmware — one reader serves both:

```text
[u32 class_count][u32 index_off]
directory: class_count × { u32 class_off, u32 link_off }   (both 4-aligned)
records:   per class: class bytes, pad to 4, link table, pad to 4
index:     class_count × IndexEntry { u32 hash, u16 idx, u16 0 }, at index_off
           (8-aligned), sorted by (hash, idx); no two entries share a hash
literals:  [u32 lit_count], then lit_count × { u32 hash, u32 off, u16 len,
           u16 flags }, right after the index (layout version 2)
```

The literal pool is the set's `String` constants, each distinct byte string
once, in order of first appearance. A row points at the bytes where they
already are, inside the constant pool of the first class that spells them;
`hash` is the string's `hashCode()`, `flags` bit 0 says the bytes are valid
UTF-8.

Two names in one set that hash alike are refused at build time (the set could
not be searched by hash alone); so are two equal names.

## Landing order and gates

| step | change | gate |
|---|---|---|
| 0 | baselines (below) | — |
| 1 | negative caching + `PRECHECK` flag + `find_class` counter | `find_class=` ≈ 0 on warm spans; `insns` identical; `builtin_dispatch` faster |
| 2 | `crates/class-link`: table, builder, validator, section, index | crate tests; every SDK class and every example papk links |
| 3 | papk v2 + framework CLSS blob + runtime cutover (`Parsed` deleted) | tests both lanes; parity counters identical; parsed metadata → 0 |
| 4 | class index + hash-based `find_method` / chain walks | `resolve_us` ↓; one `method_name` read per hit (test) |
| 5 | descriptor-first `op_invoke` hit path + smalls | `invoke_us/invokes` ↓; DWT profile |
| 6 | native dispatch: claim hint (6a); hash-matched module `match` (6b) **dropped** — see the profile | inherited natives skip the re-walk (test); 6b: ≤ 0.4 % of a graphics workload's CPU |
| 7 | this document's results | — |

## Post-commit fix: the W boards' debug image (2026-09-29)

`flash.sh --board pico_display2_w --app claudeusage` (debug: no name shrink,
line numbers, no LTO) stopped linking after f5d93245 — 77 KB over the
2048 KB program region, which the release + shrink image (1,992 KB) had
masked. The three W boards carry the radio firmware, the net stack and TLS
(~500 KB more than the testbench image), so their debug builds had ~46 KB
of headroom before the tables arrived. Fix: `app_region_kb` 1536 → 1280 on
`pico_display2_w`, `testbench_rp2350w` and `pico_enviro_mon_w` (program
region 2304 KB; eight claudeusage-sized apps still fit). The debug image is
2,173,900 B, 185 KB under the new region. Non-W boards are unchanged. Not
caught before the commit because CI links only the testbench boards and
`cargo clippy` for the others does not link — a `build.sh` row for one W
board in debug is the missing gate.

## Follow-ups (each gated by a measurement, none done here)

- **Native-path name decode** (3.3 % of a graphics workload's CPU): a native
  target decodes `(class, name, descriptor)` on every call because the
  handler API takes `&str`. Remembering the three `&'static` slices per site
  would put pointers into the pointer-free resolution tables (they survive an
  app reload only because they hold none); a handler API keyed by the
  `Methodref`'s pack-time hashes would be the clean fix and is the natural
  successor to this work. `from_utf8` alone is 1 %: the builder already
  refuses a name that is not UTF-8, so the check is redundant on a validated
  image — kept, because a corrupted image would then be undefined behaviour
  rather than `InvalidBytecode`.
- ~~**`alloc_with_defaults` chain walk**~~ — done in the second round,
  from the packer instead of a memo: see below.
- **Tag nibble packing** in the link table (−8 KB of SDK flash) and dropping
  the header words the RP2040 never reads, if the RP2040 gets tight again
  (215 KB free in the 1152K region today).
- **`checkcast` / `instanceof` / catch by class-entry hash** (tag-7 hash
  descriptors, ~7 KB SDK flash): `is_instance_of` already walks by hash, but
  the CP class name is still decoded first.
- **`ObjectHeap::class_table` keyed by class index** instead of a
  `&'static str` scan, and the `Vec<ClassFile>` replaced by the two static
  segments (−4 KB RAM on the RP2350).
- ~~**`papk-info --verify`**~~ — `papk-info` runs `validate_structure`,
  which includes the deep check, on every file it opens.

## Results

### Per step — benchmark, device, release + `--shrink --shrink-app`, plain (ms)

| section | step 0 | step 1 | step 3 | step 4 | step 5 | step 6 |
|---|---|---|---|---|---|---|
| method_dispatch | 11564 | 11481 | 11618 | 11868 | **8277** | 8279 |
| interface_dispatch | 7564 | 7561 | 8502 | 8101 | **5711** | 5707 |
| builtin_dispatch | 20989 | **7138** | 7344 | 7386 | 7447 | 7156 |
| native_static_dispatch | 2594 | 2521 | 2735 | 2538 | 2408 | 2532 |
| object_allocation | 7746 | **6379** | **12246** | **6034** | **4862** | 4655 |
| string_operations | 7613 | **4410** | **7131** | **3999** | 3891 | 3958 |
| TOTAL | 123619 | **105415** | 113716 | 104585 | 97264 | 96955 |

Step 1 (negative caching): a class-file-less call went from ~70 µs to ~24 µs;
`object_allocation` moved because every `Object.<init>` was one of those
walks; the unrelated sections sit within ±1 %, the rebuild noise floor.

Step 3 (link tables, `Parsed` gone) *regressed* `object_allocation` and
`string_operations` by ~1.9× and ~1.6×: `find_class` was still a linear
scan, and with the parsed record gone its per-class name check read the
name hash from the flash-resident table, on every `new` and every miss.
Step 4 is the cure — the name hash back in the 16 B `ClassFile`, the
sorted class index (two binary searches instead of a ~250-class scan), and
superclass/interface walks by the tables' hashes: both sections end below
their step 1 figures, and the sim's deterministic `pending-op drain` span
(1,803 insns, 122 resolutions) spends 0 µs resolving where step 3 spent
20 µs and the baseline 1 µs. `interface_dispatch` and `builtin_dispatch`
sit +3…7 % over step 1 on this image; both are warm-cache sections that
resolution no longer touches, so that is layout noise until the plain
median says otherwise (the `parity-metrics` build's counters are
unchanged between steps 3 and 4).

Step 5 (descriptor-first hit path): a resolved Java call no longer touches
the constant pool at all — the `Methodref`'s pack-time record gives the
argument count, the receiver's class id keys the probe, and the frame is
built straight from the caller's operand stack. `method_dispatch` and
`interface_dispatch` drop 30 %, `object_allocation` 19 % (every `<init>`
is such a call); the native sections are flat, as expected — a native
target still decodes the names for the handlers' string API.

Step 6a (claim hint): flat on this benchmark, whose natives are all
claimed at depth 0; what it removes is the per-call superclass re-walk on
natives inherited by app classes (`MyActivity extends Activity`, a custom
`View`, an exception subclass), pinned by the recorder test rather than
measured here.

Device figures: testbench_rp2350 firmware, release, on the bench's RP2350 slot.
"plain" is the build without `parity-metrics` — the real-world number; the
`parity-metrics` build (what `parity-bench.sh --hil` records) is ~1.7× slower
and is kept for its deterministic counters. Device wall-clock across rebuilds
carries ±5 % layout noise, so each figure is the median of same-image runs and
the deterministic counters travel with it.

### Speed — benchmark, release + `--shrink --shrink-app`, plain (ms)

| section | before (89405ab1) | after | Δ |
|---|---|---|---|
| method_dispatch (200k virtual calls) | 11,564 | 8,279 | -28.4 % |
| interface_dispatch (200k interface calls) | 7,564 | 5,707 | -24.6 % |
| builtin_dispatch (300k class-file-less virtual calls) | 20,989 | 7,156 | -65.9 % |
| native_static_dispatch (100k framework static natives) | 2,594 | 2,532 | -2.4 % |
| object_allocation | 7,746 | 4,655 | -39.9 % |
| string_operations | 7,613 | 3,958 | -48.0 % |
| array_operations | 22,116 | 22,082 | -0.2 % |
| control_flow | 6,302 | 6,210 | -1.5 % |
| int / long / float / double arithmetic (sum) | 37,125 | 36,370 | -2.0 % |
| **TOTAL** | **123,619** | **96,955** | **-21.6 %** |

Per call before: builtin ≈ 70 µs; native static ≈ 26 µs; method_dispatch
iteration ≈ 58 µs. After: builtin ≈ 24 µs; method_dispatch iteration ≈ 41 µs
(the iteration is ~10 bytecodes plus the call; the call itself lost about
17 µs). Sections resolution never touched — the arithmetic, arrays, control
flow — sit within ±1 %, which is this image's noise floor; the "after"
column is one image, and the deterministic-counter build below moves the
same way.

### Speed — benchmark, parity-metrics build (deterministic counters)

| | before | after |
|---|---|---|
| device shrink: wall / method / interface / builtin / native_static / object_alloc / string_ops (ms) | 214,443 / 18,234 / 12,419 / 25,165 / 4,374 / 9,317 / 8,390 | 184,905 / 14,165 / 10,134 / 10,867 / 4,374 / 6,899 / 5,190 (3 runs, p2p 0.000 %) — wall −13.8 % |
| device no-shrink: wall / method / interface / builtin / native_static / object_alloc / string_ops (ms) | 232,086 / 19,957 / 13,577 / 29,839 / 4,836 / 11,694 / 9,890 | 188,629 / 15,125 / 10,770 / 12,570 / 4,579 / 6,768 / 4,974 — wall −18.7 % |
| sim pico_display2_w shrink: wall / method / interface / builtin / native_static / object_alloc / string_ops (ms, median of 3) | 4,511 / 490 / 243 / 305 / 87 / 258 / 153 | 4,278 / 440 / 221 / 199 / 85 / 227 / 122 — wall −5.2 % (the host has no XIP flash to save) |
| insns / allocs / gcs (device) | 69,365,716 / 100,290 / 418 | 69,365,715 / 100,289 / 417 (steps 3–6; one alloc-failure retry fewer, see below) |

The counters are deterministic for a given image *and heap*: an allocation
that finds the heap full is retried after a collection (`retry_after_gc`),
and each retry counts one instruction, one allocation and one GC. They
therefore move with heap headroom, not only with the interpreter — the sim
reproduces the device's 715 / 289 / 417 exactly under a 330 KB limit, and
gives 651 / 225 / 391 under 480 KB. The −1 / −1 / −1 since step 3 is one
retry fewer, bought by the ~76 KB of parsed metadata the step freed; the
gate for "the interpreter executes the same program" is the counters at a
fixed heap, which hold.

### Where the CPU goes after step 6a — graphicsbench, pico_display2_w, release + shrink + shrink-app

DWT `PCSR` samples of core 0 over the bench's 12 s run (30,070 busy samples of
44,905; the sampler and its recipe are in the sram-hotpath design, Appendix B;
attribution by `nm -S` on the flashed ELF). Shares of busy time:

| group | share | what is in it |
|---|---|---|
| LVGL (C) | 48.7 % | `get_prop_core` 12.0, `lv_event_send` 7.3, `get_selector_style_prop` 5.4, blend 3.1, style lookups, fonts |
| interpreter loop | 2.5 % | `Executor::run` |
| constant-pool name decode | 3.3 % | `from_utf8` 1.0, `cp_name_and_type` 0.6, `cp_member_ref` 0.6, `cp_class_name` 0.5 — the names a *native* target still decodes for the handlers' string API, plus `ldc`/`new` class names |
| framework handler walk + memo | 1.9 % | `dispatch_module` 0.9, `PicodroidNativeHandler::dispatch` 0.6, `BuiltinHandler::dispatch` 0.5 |
| `op_invoke` | 0.8 % | the whole descriptor-first path |
| `finalize_native` / `dispatch_native` | 0.8 % | |
| graphics module dispatch | 0.4 % | `graphics::dispatch_with`, `is_view`, the per-class method matches |
| resolve cache | 0.4 % | |
| `name_eq` / `bcmp` | 0.3 % | |
| `find_class` / index | 0.2 % | |
| frame push/pop, GC | 0.2 %, 0.2 % | |

Everything the plan's step 6b would have touched — the graphics class `match`,
`is_view`/`is_view_group`, the string compares — is under half a percent of
the CPU on the workload it was meant for; the rendering it feeds is a
hundred times that. 6b is dropped. The one interpreter-side item left with a
visible share is the native path's name decode (3.3 %): a `&str` API on the
handler side needs the names, and caching them per site would put pointers
into the pointer-free resolution tables. Noted as a follow-up, not done.

### Memory — claudeusage History page

| | before | after | Δ |
|---|---|---|---|
| sim census: classes parsed / parsedB (host) / devB~ (device model) / class table | 113 of 280 / 76,768 / 75,864 / 5,612 | 280 registered, nothing parsed / 0 / 0 / 4,492 (16 B a class; 3,372 at step 3's 12 B) | −75,864 −1,120 |
| sim snapshot: live (obj / arr / str), nused, nfree, largest block | 17,012 (12,068 / 4,312 / 632), 266,200, 151,592, 108,048 | 17,060 (12,116 / 4,312 / 632), 185,112, 232,680, 215,736 | arena in use **−81,088**; largest free block ×2 |
| device `pdb sysmon`, boot page: free heap / min free / JVM live / post-GC floor / largest free | 115,824 / 111,376 / 21,347 / 11,934 / 99,136 | 168,192 / 158,632 / 21,299 / 11,934 / 139,776 | free heap **+52,368**, min free +47,256, largest free +40,640; JVM live ±0 |
| device `pdb sysmon`, after 3 × key B: free heap / min free / JVM live | 117,456 / 111,376 / 18,913 | 169,912 / 158,632 / 18,865 | free heap **+52,456** |

### Flash and static RAM (size lane, helloworld, release)

| | before | after | Δ |
|---|---|---|---|
| rp2040 flash / ram | 905,156 / 246,692 (896K region) | did not link in the 896K region (overflow ~40 KB); `app_region_kb` 1024 → 768 gives 1152K: 959,660 (step 3), 962,436 (step 4), 964,092 (step 5), 964,704 / 246,684 (step 6a) | +59,548 flash, RAM −8 |
| rp2350 flash / ram | 1,355,624 / 414,336 | 1,476,920 (step 3); 1,478,636 (step 4); 1,479,996 (step 5); 1,480,560 / 414,336 (step 6a) | +124,936 flash, RAM ±0 |
| benchmark.papk / claudeusage.papk (shrink + shrink-app) | 5,368 B / ~99 KB | 6,992 B / 118,004 B | +30 % / +19 %: the link tables and the index |
| framework class section in firmware (rp2350 `testbench_rp2350`, 246 classes) | — (raw class files) | 103,056 B of tables + ~4 KB directory and index over the class bytes | the flash the RAM came from |

(The committed ratchet was 903,824 / 1,354,080; the tree measured +1.3 / +1.5 KB
above it before this work began.)

## Second round (2026-09-30): what else the packer can take over

The first round stored offsets and hashes and resolved nothing. This round
asked which of the remaining run-time work is fixed at pack time, measured
each candidate first, and built the ones the numbers supported.

### Measurement first

Counters under `parity-metrics` (`packtime:` line, printed with every span
report and at exit): `ldc` executions and how many produced a `String`,
`invokedynamic`, `checkcast` + `instanceof`, `new`, each with its time, and
the flash-resident strings `intern` had to copy to the heap. Boot and launch
print how long the package scan, each system app's validation, class
registration and the asset registry took.

`pico_touch_kit` (RP2350B), release + `--shrink --shrink-app`,
`parity-metrics` build (each timed op carries about 1–2 µs of clock reads):

| per operation, before | qa_ui | langsuite_kt_stdlib |
|---|---|---|
| `ldc` (77 % / 96 % of them strings) | 19.1 µs | 33.6 µs |
| `new` | 36.4 µs | 46.6 µs |
| `checkcast` / `instanceof` | 17 µs (63 of them) | 19.4 µs (2,975 of them) |
| `invokedynamic` | 196 µs (67) | 139 µs (17) |
| average bytecode, for scale | ≈ 1.5 µs | |

At boot, with two system apps and one installed app: system-app validation
1.8 ms (launcher, 10 KB) + 9.0 ms (settings, 37 KB); the region scan 27 ms
for a 97 KB app and 36 ms for a 163 KB one — about 0.25 ms per KB of PAPK,
nearly all of it the deep link-table check. Registering ~280 classes takes
1.3–1.6 ms and the asset registry 0.1–0.2 ms.

What that ruled in and out:

- A string `ldc` cost 13–22 average bytecodes: a linear byte-compare against
  every interned string, inside a scheduler-atomic section, and — once the
  app had built its first dynamic string — a heap copy of every literal not
  seen before (705 copies, 8.8 KB, in the Kotlin suite). **Built.**
- `new` cost 24–31 average bytecodes. **Built.**
- Type checks cost as much per operation but only a Kotlin-heavy workload
  runs many (11 % of the Kotlin suite's time, under 0.1 % of qa_ui).
  **Not built here** — see "Left for a decision".
- `invokedynamic` is slow but rare: 13 ms of a 2 s run (0.6 %). Not built.
- Class registration and the asset registry are not worth touching.

### What was built

**A literal pool per class section.** The section builder deduplicates every
`String` constant of the set into a pool after the index; each class's
`StringDesc` carries its row. The runtime gives the top of the `u16`
string-reference range to the two pools — the framework's, then the app's,
ending at `0xFFFF` — so an `ldc` of a string is `pool base + row`: no entry
in `StringTable`, no search, no atomic section, no copy
(`StringTable::set_literal_pools`, `literal_ref`; `helpers::resolve_ldc`).
The table's own entries still start at 0 and may not grow into the pools.
A literal's `hashCode()` is read from its row. A constant the framework and
the app both spell has a row in each pool, so `if_acmpeq` / `if_acmpne`
treat two literal references with equal bytes as one object
(`StringTable::same_literal`, JLS §3.10.5). The collector never marks or
sweeps a literal. A class linked outside a section (tests, `link-at-load`)
carries `LIT_NONE` and interns as before.

**Instance layouts.** Each `FieldInfo` now records the field's slot among
its class's own fields and its kind (reference, int-like, float, long,
double); the header records how many slots the class's own fields take and
where its superclass sits in the section. `new` on a site the resolution
tables know (`ObjectHeap::alloc_instance`) walks the chain by index — the
only lookup left is the step from an app class to its framework parent —
sums the slot counts, and writes typed zeros for the primitive fields in one
pass. It decodes no class name, no superclass name and no descriptor, and
allocates no `Vec` for the chain. `op_new` probes its site before reading
the constant pool, and `ObjectHeap::intern_class` looks a name up by pointer
before comparing bytes. `helpers::super_index` — every method and field
walk — uses the same index.

**System apps are not deep-checked at boot.** A system app is part of the
firmware image, and the build script refuses to embed one that fails
`validate_structure`. `packages::register_system` now runs
`papk_format::validate_embedded` on a device: the header, the manifest walk
and the class section's bounds. The simulator, which takes its system apps
from files at run time, still checks them in full.

### Results

Same board and build as above.

| | before | after | Δ |
|---|---|---|---|
| qa_ui: `ldc`, total / each | 94.1 ms / 19.1 µs | 21.2 ms / 4.3 µs | −77 % |
| qa_ui: `new`, total / each | 136.9 ms / 36.4 µs | 68.8 ms / 18.3 µs | −50 % |
| qa_ui: allocations / collections | 13,774 / 78 | 13,347 / 74 | −427 / −4 |
| langsuite_kt_stdlib: `ldc`, total / each | 71.7 ms / 33.6 µs | 11.0 ms / 5.2 µs | −85 % |
| langsuite_kt_stdlib: `new`, total / each | 49.8 ms / 46.6 µs | 34.1 ms / 31.9 µs | −32 % |
| langsuite_kt_stdlib: allocations / collections | 5,095 / 19 | 4,390 / 17 | −705 / −2 |
| literals copied to the heap (either app) | 402 / 705 | 0 | |
| boot: system-app validation (launcher + settings) | 10.8 ms | 0.05 ms | −10.7 ms |
| boot: region scan, 163 KB → 175 KB Kotlin app | 35.6 ms | 48.2 ms | **+12.6 ms** |
| bytecodes executed | | unchanged | |

The whole-run effect on a UI workload is small, as the first round's profile
predicted: qa_ui's 2 s handler span went 1,971 → 1,946 ms, which is inside
its run-to-run spread, because 45 % of it is inside native calls (LVGL).
The interpreter columns are where it shows.

Costs:

| | before | after | Δ |
|---|---|---|---|
| rp2350 flash (`testbench_rp2350`) | 1,480,020 | 1,487,932 | +7,912 |
| rp2040 flash (`testbench_rp2040`) | 965,724 | 970,564 | +4,840 (208.8 KB free) |
| static RAM, both | | | ±0 |
| qa_ui.papk / langsuite_kt_stdlib.papk | 44,612 / 162,944 | 50,336 / 175,096 | +12.8 % / +7.5 % |
| framework pool | — | 383 rows for 438 `String` entries | |

An app grows by 12 B per distinct string constant, 4 B per `String` entry,
2 B per field and 8 B per class. The boot scan of an installed app got
slower with it (the pool and the superclass indices are validated too),
which more than cancels the system-app saving for a large string-heavy app.

### Left for a decision

- **Installed apps are deep-checked on every boot**, ~0.28 ms per KB: 48 ms
  for one 175 KB app, and it grows with everything installed (the touch
  kit's app region is 9.9 MB). Checking once when an install commits and
  recording that in the run's boot-meta flags would make the boot scan a
  header read per run. It changes what the device trusts after a flash
  fault, which is why it is not done here; the app-store roadmap's S3
  (section CRCs and a signature) is the natural place for it.
- **Type checks.** `checkcast` / `instanceof` decode the target name, hash
  it and the runtime class's name, and walk the hierarchy through string
  tables, every time, uncached. Two ways to fix it: a small table in the
  resolution cache keyed by (site, runtime class id) → yes/no (about 60
  lines, answers builtin classes too, costs 256–512 B of RAM), or
  class-entry hashes plus per-class ancestor lists from the packer (no RAM,
  ~7 KB of framework flash, and the classfile-less builtins still need
  their tables). The first matches how virtual calls are already handled.
- **`invokedynamic`**: a per-site record (target, capture count, parameter
  kinds) and one cached proxy for a non-capturing lambda. 0.6 % today.
- **Resource strings** (`getString`) still copy to the heap on every call;
  a third flash-backed reference range would serve them as the pools serve
  literals. Only apps with `res/` string tables benefit.
- **`ConstantValue` is never read.** A `static final` compile-time constant
  has no `putstatic` in `<clinit>`; javac and kotlinc inline such constants
  at every use, so no `getstatic` of one has been seen, but a class from
  another compiler that reads one would see the type's default.
