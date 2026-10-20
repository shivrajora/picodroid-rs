// SPDX-License-Identifier: GPL-3.0-only
//! (Re)writes the seed inputs under `fuzz/regress/<target>/seed-*`. Ignored
//! by default; the seeds are committed, and this exists so they can be
//! regenerated when a format changes:
//!
//!     cargo test -p fuzz-targets --test seeds -- --ignored write_seeds
//!
//! It reads the committed golden fixtures, plus `build/apks/helloworld.papk`
//! and helloworld's compiled class when a build left them there.
//!
//! The TLS seed is a real server's half of a handshake, captured from a
//! local `openssl s_server` (see `capture_tls_seed`); a seed that reaches
//! a ServerHello lets libFuzzer mutate from inside the record layer instead
//! of rediscovering the framing.

use std::io::{Read as _, Write as _};
use std::path::{Path, PathBuf};

use class_link::build::{minimal_class, simple_class};
use papk_format::res::layout;
use papk_format::{AlignedBuf, AssetSpec, EntryPoint, ManifestSpec, Papk, PapkBuilder};
use pd_codec::{sandbox, sntp, time, wifi};
use pdb_protocol::greeting::{self, AppsInfo, GREETING_MAX};
use pdb_protocol::input::{InputEvent, MAX_INPUT_PAYLOAD};
use pdb_protocol::sysmon::{self, SysmonSample, RESPONSE_MAX};

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn put(target: &str, name: &str, bytes: &[u8]) {
    let dir = root().join("fuzz/regress").join(target);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join(format!("seed-{name}")), bytes).unwrap();
}

fn read_opt(path: &Path) -> Option<Vec<u8>> {
    std::fs::read(path).ok()
}

fn papk_with(fmv: &str, classes: &[(&str, &[u8])], features: Option<&str>) -> Vec<u8> {
    let mut b = PapkBuilder::new(ManifestSpec {
        entry: EntryPoint::MainClass("seed/Main"),
        package_name: "seed.app",
        version: "1.0",
        framework_map_version: fmv,
        version_code: Some(3),
        label: Some("Seed"),
        icon: Some("dot.png"),
        design_size: Some((320, 240)),
        requires_features: features,
    });
    for (name, blob) in classes {
        b.class(name, blob);
    }
    b.asset(AssetSpec {
        name: "dot.png",
        width: 2,
        height: 2,
        cf: 0x12,
        stride: 0,
        data: &[1, 2, 3, 4, 5, 6, 7, 8],
    });
    b.build().unwrap()
}

#[test]
#[ignore]
fn write_seeds() {
    let fixtures = root().join("crates/papk-format/tests/fixtures");
    let minimal = std::fs::read(fixtures.join("minimal.papk")).unwrap();
    let with_assets = std::fs::read(fixtures.join("with-assets.papk")).unwrap();
    let main_class = std::fs::read(fixtures.join("Main.class")).unwrap();
    let hello = read_opt(&root().join("build/apks/helloworld.papk"));
    let hello_class = read_opt(
        &root().join("examples/helloworld/build/classes/java/main/helloworld/HelloWorld.class"),
    );
    let code: &[u8] = &[
        0x03, 0x3B, 0x1A, 0x9A, 0x00, 0x05, 0x84, 0x00, 0x01, 0x1A, 0xAC,
    ];
    let main_a = simple_class(b"seed/Main", b"java/lang/Object", Some(code));
    let main_b = minimal_class(b"seed/Other");
    let built = papk_with(
        "0.40.0",
        &[("seed/Main", &main_a), ("seed/Other", &main_b)],
        None,
    );
    let unshrunk = papk_with(
        "0.0.0",
        &[("seed/Main", &main_a)],
        Some("fuzz.hardware.present"),
    );

    // ── PAPK ──────────────────────────────────────────────────────────────
    let mut papks = vec![
        ("minimal", minimal.clone()),
        ("with-assets", with_assets.clone()),
        ("built", built.clone()),
        ("unshrunk", unshrunk.clone()),
    ];
    if let Some(h) = &hello {
        papks.push(("helloworld", h.clone()));
    }
    for (name, p) in &papks {
        put("papk", name, p);
        let mut cut = 512u16.to_le_bytes().to_vec();
        cut.extend_from_slice(p);
        put("papk_prefix", name, &cut);
        let section = Papk::parse(&AlignedBuf::new(p))
            .unwrap()
            .class_section()
            .unwrap()
            .as_bytes()
            .to_vec();
        put("class_section", name, &section);
    }
    put("records", "flash-meta", &{
        let mut v = vec![1u8];
        v.extend_from_slice(&papk_format::flash_image::build_meta_pages(1000, 1, 2));
        v
    });
    put("records", "versions", b"\x000.40.0\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x000.41.2");
    put("install", "shrunk", &[&[1u8][..], &built].concat());
    put("install", "unshrunk", &[&[0u8][..], &unshrunk].concat());
    put("install", "minimal", &[&[0u8][..], &minimal].concat());
    put(
        "papk_roundtrip",
        "a",
        b"\x08\x00seed/App\x08\x00seed.app\x03\x001.0\x06\x000.40.0\x01\x04\x00Seed\x01\x03\x00\x00\x00\x00",
    );
    put("papk_roundtrip", "zeros", &[0u8; 64]);

    // ── Classes ───────────────────────────────────────────────────────────
    put("link_class", "main", &main_class);
    put("link_class", "simple", &main_a);
    if let Some(c) = &hello_class {
        put("link_class", "helloworld", c);
    }
    put("bytecode_check", "loop", code);
    put("bytecode_check", "return", &[0x03, 0xAC]);
    // iload_0; tableswitch (padded) default +28, 0..1 -> +28, +28; iconst_0; ireturn
    put(
        "bytecode_check",
        "tableswitch",
        &[
            0x1A, 0xAA, 0, 0, 0, 0, 0, 28, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 28, 0, 0, 0, 28, 0x00,
            0x00, 0x00, 0x00, 0x03, 0xAC,
        ],
    );

    // ── Interpreter: forward-only code that does some work ───────────────
    put("interp", "return", &[0x03, 0xAC]);
    // iconst_5; newarray int; arraylength; iconst_2; idiv; ireturn
    put(
        "interp",
        "array",
        &[0x08, 0xBC, 0x0A, 0xBE, 0x05, 0x6C, 0xAC],
    );
    // iconst_1; ifeq +6; bipush 40; ireturn; iconst_0; ireturn
    put(
        "interp",
        "branch",
        &[0x04, 0x99, 0x00, 0x06, 0x10, 0x28, 0xAC, 0x03, 0xAC],
    );
    // iconst_0; iconst_0; idiv (ArithmeticException); ireturn
    put("interp", "throws", &[0x03, 0x03, 0x6C, 0xAC]);

    // ── pdb ───────────────────────────────────────────────────────────────
    for (name, e) in [
        (
            "key",
            InputEvent::Key {
                keycode: 4,
                meta: 0,
            },
        ),
        ("tap", InputEvent::Tap { x: 160, y: 120 }),
        (
            "swipe",
            InputEvent::Swipe {
                x1: 0,
                y1: 0,
                x2: 319,
                y2: 239,
                duration_ms: 300,
            },
        ),
    ] {
        let mut out = [0u8; MAX_INPUT_PAYLOAD];
        let n = e.encode(&mut out);
        put(
            "pdb_decode",
            &format!("input-{name}"),
            &[&[0u8][..], &out[..n]].concat(),
        );
    }
    let mut g = [0u8; GREETING_MAX];
    let apps = AppsInfo {
        max: 8,
        installed: 2,
        largest_free: 65536,
        total_free: 131072,
    };
    let n = greeting::encode_with_apps(
        1 << 20,
        b"0.40.0",
        b"testbench_rp2350",
        b"E6614103E7212F2B",
        &apps,
        &mut g,
    );
    put("pdb_decode", "greeting", &[&[1u8][..], &g[..n]].concat());
    let mut s = SysmonSample {
        uptime_ticks: 1000,
        task_count: 2,
        ..Default::default()
    };
    s.tasks[0].name[..3].copy_from_slice(b"jvm");
    s.tasks[1].name[..4].copy_from_slice(b"IDLE");
    let mut out = [0u8; RESPONSE_MAX];
    let n = sysmon::encode(&s, None, &mut out);
    put("pdb_decode", "sysmon", &[&[2u8][..], &out[..n]].concat());
    put(
        "pdb_decode",
        "greeting-fields",
        b"\x03\x00\x00\x10\x00\x06\x000.40.0\x10\x00testbench_rp2350\x04\x00ABCD\x01\x08\x02",
    );

    // ── HTTP ──────────────────────────────────────────────────────────────
    put(
        "http_head",
        "length",
        b"\x05\x07\x03HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: 5\r\n\r\nhello",
    );
    put(
        "http_head",
        "chunked",
        b"\x40\x40\x40HTTP/1.1 404 Not Found\r\nTransfer-Encoding: gzip, chunked\r\n\r\n",
    );
    put(
        "http_chunked",
        "wire",
        b"\x00a;ext=1\r\n0123456789\r\n5\r\nhello\r\n0\r\nTrailer: x\r\n\r\n",
    );
    put(
        "http_chunked",
        "framed",
        b"\x01\x02\x00\x03\x00\x07\x00the quick brown fox",
    );

    // ── pdb stream: [config][frames…] ────────────────────────────────────
    let framed = |cmd: u8, payload: &[u8]| -> Vec<u8> {
        let mut v = pdb_protocol::FRAME_MAGIC.to_vec();
        v.push(cmd);
        v.extend_from_slice(&(payload.len() as u32).to_le_bytes());
        v.extend_from_slice(payload);
        v.extend_from_slice(
            &pdb_protocol::crc32_frame(cmd, payload.len() as u32, payload).to_le_bytes(),
        );
        v
    };
    let multi = 0b011u8; // sysmon available, multi-app
    put(
        "pdb_stream",
        "ping",
        &[&[multi][..], &framed(pdb_protocol::CMD_PING, b"")].concat(),
    );
    put(
        "pdb_stream",
        "sysmon-twice",
        &[
            &[multi][..],
            &framed(pdb_protocol::CMD_SYSMON, b""),
            &framed(pdb_protocol::CMD_SYSMON, b""),
        ]
        .concat(),
    );
    put(
        "pdb_stream",
        "list",
        &[&[multi | 4][..], &framed(pdb_protocol::CMD_LIST, b"")].concat(),
    );
    put(
        "pdb_stream",
        "list-single-app",
        &[&[0u8][..], &framed(pdb_protocol::CMD_LIST, b"")].concat(),
    );
    for (name, e) in [
        (
            "input-key",
            InputEvent::Key {
                keycode: 4,
                meta: 0,
            },
        ),
        ("input-tap", InputEvent::Tap { x: 160, y: 120 }),
    ] {
        let mut out = [0u8; MAX_INPUT_PAYLOAD];
        let n = e.encode(&mut out);
        put(
            "pdb_stream",
            name,
            &[&[multi][..], &framed(pdb_protocol::CMD_INPUT, &out[..n])].concat(),
        );
    }
    put(
        "pdb_stream",
        "uninstall-com-a",
        &[
            &[multi | 4][..],
            &framed(pdb_protocol::CMD_UNINSTALL, b"com.a"),
            &framed(pdb_protocol::CMD_LIST, b""),
        ]
        .concat(),
    );
    put(
        "pdb_stream",
        "garbage-then-ping",
        &[
            &[multi][..],
            b"xPD\xff",
            &framed(pdb_protocol::CMD_PING, b""),
        ]
        .concat(),
    );
    put(
        "pdb_stream",
        "false-start",
        &[&[multi][..], b"PD", &framed(pdb_protocol::CMD_PING, b"")].concat(),
    );
    put("pdb_stream", "zeros", &[multi, 0, 0, 0, 0, 0, 0, 0, 0]);
    let mut oversized = pdb_protocol::FRAME_MAGIC.to_vec();
    oversized.push(pdb_protocol::CMD_INPUT);
    oversized.extend_from_slice(&u32::MAX.to_le_bytes());
    put(
        "pdb_stream",
        "oversized-then-ping",
        &[
            &[multi][..],
            &oversized,
            &framed(pdb_protocol::CMD_PING, b""),
        ]
        .concat(),
    );
    let mut bad_crc = framed(pdb_protocol::CMD_PING, b"");
    let n = bad_crc.len();
    bad_crc[n - 1] ^= 0xFF;
    put(
        "pdb_stream",
        "bad-crc-ping",
        &[&[multi][..], &bad_crc].concat(),
    );
    // An install: header, then the PAPK with its CRC as `MemTransport`
    // frames it, then a PING.
    let mut inst = pdb_protocol::FRAME_MAGIC.to_vec();
    inst.push(pdb_protocol::CMD_INSTALL);
    inst.extend_from_slice(&(built.len() as u32).to_le_bytes());
    inst.extend_from_slice(&built);
    inst.extend_from_slice(
        &pdb_protocol::crc32_frame(pdb_protocol::CMD_INSTALL, built.len() as u32, &built)
            .to_le_bytes(),
    );
    inst.extend_from_slice(&framed(pdb_protocol::CMD_PING, b""));
    put(
        "pdb_stream",
        "install-built",
        &[&[multi][..], &inst].concat(),
    );

    // ── rescan: [tail choice][mode][ops…] ────────────────────────────────
    // Ops: 0 bake (sector, image, len variant, flags, seq u32, bits),
    // 1 erase, 2 scribble, 3 header only, 4 copy sector, 5 flip a bit.
    put(
        "rescan",
        "two-runs",
        &[
            0, 1, //
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 3, // com.a at 0, committed + verified
            0, 3, 0, 1, 1, 0, 1, 0, 0, 0, 1, // com.b (two sectors) at 3, committed
        ],
    );
    put("rescan", "stale", &[0, 1, 0, 5, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
    put(
        "rescan",
        "duplicate",
        &[
            0, 1, //
            0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 1, // com.a at 0, seq 1
            0, 4, 0, 0, 0, 0, 2, 0, 0, 0, 1, // com.a at 4, seq 2
        ],
    );
    put(
        "rescan",
        "collide",
        &[1, 1, 0, 0, 0, 2, 0, 0, 0, 0, 0, 0, 1],
    );
    put("rescan", "raw-erased", &[0, 0, 0xFF, 0xFF, 0xFF, 0xFF]);
    let mut raw = vec![0u8, 0];
    raw.extend_from_slice(&papk_format::flash_image::build_meta_pages(
        minimal.len() as u32,
        0,
        1,
    ));
    raw.extend_from_slice(&[0xFF; 4096 - 512]);
    raw.extend_from_slice(&minimal);
    put("rescan", "raw-header", &raw);

    // ── HTTP connection: [flags][u16 n][sizes][u16 m][sizes][wire] ───────
    let conn = |flags: u8, socket: &[u8], app: &[u8], wire: &[u8]| -> Vec<u8> {
        let mut v = vec![flags];
        v.extend_from_slice(&(socket.len() as u16).to_le_bytes());
        v.extend_from_slice(socket);
        v.extend_from_slice(&(app.len() as u16).to_le_bytes());
        v.extend_from_slice(app);
        v.extend_from_slice(wire);
        v
    };
    put(
        "http_conn",
        "python-length",
        &conn(
            1,
            &[7, 64],
            &[128],
            b"HTTP/1.0 200 OK\r\nServer: SimpleHTTP/0.6 Python/3.12\r\nDate: Sun, 19 Oct 2026 20:00:00 GMT\r\nContent-type: text/plain\r\nContent-Length: 13\r\nLast-Modified: Sun, 19 Oct 2026 19:00:00 GMT\r\n\r\nHello, World!",
        ),
    );
    put(
        "http_conn",
        "chunked-trailers",
        &conn(
            0,
            &[2, 15],
            &[0, 3],
            b"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nTransfer-Encoding: chunked\r\n\r\n4\r\nWiki\r\n5;ext=1\r\npedia\r\n0\r\nX-Trailer: v\r\n\r\n",
        ),
    );
    put(
        "http_conn",
        "no-length-eof",
        &conn(
            1,
            &[],
            &[],
            b"HTTP/1.0 200 OK\r\n\r\nread until the server closes",
        ),
    );
    put(
        "http_conn",
        "204",
        &conn(0, &[], &[], b"HTTP/1.1 204 No Content\r\n\r\n"),
    );
    let mut big = Vec::from(&b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n"[..]);
    while big.len() < 70 {
        big.extend_from_slice(b"X-Pad: aaaaaaaaaaaa\r\n");
    }
    big.extend_from_slice(b"\r\n3\r\nabc\r\n0\r\n\r\n");
    put("http_conn", "big-head-chunked", &conn(0, &[31], &[7], &big));
    put(
        "http_conn",
        "truncated",
        &conn(
            0,
            &[],
            &[],
            b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n4\r\nWi",
        ),
    );
    put(
        "http_conn",
        "socket-fails",
        &conn(
            3,
            &[1, 5],
            &[3],
            b"HTTP/1.1 200 OK\r\nContent-Length: 5\r\n\r\nhello",
        ),
    );

    // ── JSON ──────────────────────────────────────────────────────────────
    put(
        "json",
        "object",
        &[
            &[0u8][..],
            r#"{"name":"picodroid","temps":[21.5,-3,1e3,0.1],"ok":true,"none":null,"esc":"a\"é\n"}"#
                .as_bytes(),
        ]
        .concat(),
    );
    put("json", "array", b"\x01[1,[2,[3,[4]]],{\"k\":\"v\"}]");

    // ── JSON ops: a document, then a script of put/remove/set/prune ───────
    // Each op byte picks the call (`json_ops`'s `% 11`); a node pick is a
    // garbage flag byte (1 = a live node) and a `u16` index into the live
    // set; a key or string is a `u16` length and the bytes.
    let doc = br#"{"a":[1,2],"b":{"c":"d"}}"#;
    let mut ops: Vec<u8> = (doc.len() as u16).to_le_bytes().to_vec();
    ops.extend_from_slice(doc);
    ops.extend_from_slice(&[0, 6]); // alloc {}
    ops.extend_from_slice(&[0, 7]); // alloc []
    ops.extend_from_slice(&[2, 1, 0, 0, 1, 7, 0, 1, 0, b'x']); // root.put("x", new {})
    ops.extend_from_slice(&[5, 1, 8, 0, 1, 1, 0, 0]); // new [].append(a[1])
    ops.extend_from_slice(&[8, 3, 1, 8, 0]); // bind slot 3 -> new []
    ops.extend_from_slice(&[2, 1, 7, 0, 1, 0, 0, 1, 0, b'r']); // {}.put("r", root): Cycle
    ops.extend_from_slice(&[4, 1, 0, 0, 1, 0, b'a']); // root.remove("a")
    ops.extend_from_slice(&[9, 0xFF]); // prune, every slot alive
    ops.extend_from_slice(&[9, 0x08]); // prune, only slot 3 alive
    put("json_ops", "script", &ops);
    put(
        "json_ops",
        "empty",
        &[
            0, 0, 0, 7, 5, 1, 0, 0, 1, 0, 0, 1, 9, 0, 0, 7, 1, 0, 0, 3, 0, 10, 5,
        ],
    );

    // ── TLS ───────────────────────────────────────────────────────────────
    for entry in std::fs::read_dir(root().join("crates/pd-tls/roots")).unwrap() {
        let path = entry.unwrap().path();
        let stem = path.file_stem().unwrap().to_string_lossy().into_owned();
        let der = std::fs::read(&path).unwrap();
        put("der", &stem, &der);
        // A root sent as its own one-entry chain: anchored at itself, so
        // the inner verifier runs its signature and date checks.
        put(
            "tls_verify",
            &format!("root-{stem}"),
            &tls_verify_seed(1_760_000_000, "example.com", &[&der]),
        );
    }
    // A real chain, leaf first, ending in a cross-signed copy of the root
    // (crates/fuzz-targets/tests/fixtures/chain-*/N.der; refetch with
    // `openssl s_client -showcerts` and `openssl x509 -outform DER`). The
    // clock is the capture time, inside the leaf's validity, so this seed
    // is the one accepted chain in the corpus.
    let chain_dir = root().join("crates/fuzz-targets/tests/fixtures/chain-api.anthropic.com");
    let chain: Vec<Vec<u8>> = (0..3)
        .map(|i| std::fs::read(chain_dir.join(format!("{i}.der"))).unwrap())
        .collect();
    let refs: Vec<&[u8]> = chain.iter().map(Vec::as_slice).collect();
    put(
        "tls_verify",
        "anthropic-ok",
        &tls_verify_seed(1_792_473_544, "api.anthropic.com", &refs),
    );
    put(
        "tls_verify",
        "anthropic-wrong-host",
        &tls_verify_seed(1_792_473_544, "example.com", &refs),
    );
    put(
        "tls_verify",
        "anthropic-expired",
        &tls_verify_seed(1_700_000_000, "api.anthropic.com", &refs),
    );
    put(
        "tls_verify",
        "anthropic-no-clock",
        &tls_verify_seed(0, "api.anthropic.com", &refs),
    );
    put(
        "tls_verify",
        "anthropic-leaf-only",
        &tls_verify_seed(1_792_473_544, "api.anthropic.com", &refs[..1]),
    );
    put(
        "tls_verify",
        "garbage-entry",
        &tls_verify_seed(
            1_792_473_544,
            "api.anthropic.com",
            &[&[0x30, 0x00], refs[1]],
        ),
    );
    put(
        "tls_verify",
        "ip-host",
        &tls_verify_seed(1_792_473_544, "127.0.0.1", &refs),
    );

    // ── pd-codec: SNTP, the /system/* records, the sandbox, a layout ──────
    let nonce: u64 = 0x0102_0304_0506_0708;
    let mut good = vec![0u8; sntp::PACKET_LEN];
    good[0] = 0x24; // LI 0, version 4, mode 4
    good[1] = 2;
    good[24..32].copy_from_slice(&nonce.to_be_bytes());
    good[40..44].copy_from_slice(&(1_791_540_000u32 + 2_208_988_800u32).to_be_bytes());
    good[44..48].copy_from_slice(&(1u32 << 31).to_be_bytes());
    let mut seed = nonce.to_le_bytes().to_vec();
    seed.extend_from_slice(&100u16.to_le_bytes());
    put("sntp", "reply", &[&seed[..], &good].concat());
    put(
        "sntp",
        "request",
        &[&seed[..], &sntp::request(nonce)].concat(),
    );
    put("sntp", "short", &[&seed[..], &good[..20]].concat());

    let c =
        wifi::Credentials::new(b"Home Net", b"correct horse", wifi::Security::Wpa2Wpa3).unwrap();
    let mut out = [0u8; wifi::ENCODED_MAX];
    let n = wifi::encode(&c, &mut out);
    put("sysfmt", "wifi-record", &[&[0u8][..], &out[..n]].concat());
    put(
        "sysfmt",
        "wifi-build",
        b"\x01\x08\x00Home Net\x0d\x00correct horse\x05\x03\x00\x02",
    );
    let r = time::Record {
        offset_minutes: 330,
        auto_time: false,
    };
    put("sysfmt", "time-record", &[&[2u8][..], &r.encode()].concat());
    put("sysfmt", "zone-id", b"\x03GMT+05:30");
    put("sysfmt", "zone-bare", b"\x03-0330");
    put(
        "sysfmt",
        "serial-id",
        b"\x04\x00\x08\x00\xe6\x63\x38\x61\xa3\x7b\x90\x38",
    );
    put("sysfmt", "serial-ascii", b"\x04\x01emulator-4242");

    put("sandbox", "nested", b"\x01\x05\x00com.a/prefs/../x");
    put("sandbox", "dots", b"\x01\x05\x00com.a//./a/./b/...");
    put("sandbox", "no-package", b"\x00\x00\x00/prefs/../x");
    let long = format!("/{}", "a".repeat(sandbox::APP_PATH_MAX));
    put(
        "sandbox",
        "too-long",
        &[b"\x01\x01\x00a", long.as_bytes()].concat(),
    );

    let node = |cls: u8, attrs: &[(u32, u32)], children: u32| -> Vec<u8> {
        let mut v = (cls as u32 | ((attrs.len() as u32) << 8) | (children << 16))
            .to_le_bytes()
            .to_vec();
        for (a, x) in attrs {
            v.extend_from_slice(&a.to_le_bytes());
            v.extend_from_slice(&x.to_le_bytes());
        }
        v
    };
    let mut layout = node(
        layout::class::FRAME_LAYOUT,
        &[
            (layout::attr::LAYOUT_WIDTH, -1i32 as u32),
            (layout::attr::LAYOUT_HEIGHT, -1i32 as u32),
            (layout::attr::BACKGROUND, 0xFF20_2020),
        ],
        2,
    );
    layout.extend(node(
        layout::class::TEXT_VIEW,
        &[
            (layout::attr::TEXT, 0x7f03_0001),
            (layout::attr::TEXT_COLOR, 0xFF11_2233),
            (layout::attr::SINGLE_LINE, 1),
            (layout::attr::LAYOUT_GRAVITY, 0x55),
            (layout::attr::LAYOUT_MARGIN_RIGHT, 8),
            (layout::attr::LAYOUT_MARGIN_BOTTOM, 4),
            (layout::attr::PADDING_LEFT, 6),
        ],
        0,
    ));
    layout.extend(node(
        layout::class::BUTTON,
        &[
            (layout::attr::TEXT, 0x7f03_0002),
            (layout::attr::LAYOUT_WEIGHT, 1.5f32.to_bits()),
            (layout::attr::BACKGROUND_RADIUS, 4),
        ],
        0,
    ));
    put("layout_node", "frame-with-two", &layout);

    // ── protobuf: [n_ops][u16 len][bytes][ops…] ──────────────────────────
    let pb = |ops: &[u8], bytes: &[u8]| -> Vec<u8> {
        let mut v = vec![ops.len() as u8];
        v.extend_from_slice(&(bytes.len() as u16).to_le_bytes());
        v.extend_from_slice(bytes);
        v.extend_from_slice(ops);
        v
    };
    // varint 300, varint 0x7F, then a read off the end.
    put("protobuf", "varint", &pb(&[1, 1, 1], &[0xAC, 0x02, 0x7F]));
    // A ten-byte -1 as int64, then the same bytes as int32 from the start.
    put(
        "protobuf",
        "negative",
        &pb(
            &[2, 9, 0, 1],
            &[0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0x01],
        ),
    );
    // Tag, skip, tag, skip… over every wire type, then a varint.
    put(
        "protobuf",
        "skip-all-wire-types",
        &pb(
            &[0, 6, 0, 0, 6, 1, 0, 6, 2, 0, 6, 5, 0, 1],
            &[
                0x08, 0x80, 0x80, 0x80, 0x80, 0x80, 0x20, 0x11, 7, 0, 0, 0, 0, 0, 0, 0, 0x1A, 3, 0,
                0, 0, 0x25, 0xAB, 0, 0, 0, 0x28, 0x4D,
            ],
        ),
    );
    // pushLimit(1), read inside it, a read that hits the limit, popLimit.
    put(
        "protobuf",
        "limits",
        &pb(&[7, 1, 0, 1, 0, 8, 0, 1], &[0x08, 0x01, 0x10, 0x02]),
    );
    // Write round trips: varint 300, int -1, a double, a fixed32.
    put(
        "protobuf",
        "roundtrip",
        &pb(
            &[
                11, 0, 0x2C, 1, 0, 0, 0, 0, 0, 0, 8, 0, 0, 0, //
                11, 1, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 12, 0, 0, 0, //
                11, 4, 0, 0, 0, 0, 0, 0, 0xF8, 0x3F, 8, 0, 0, 0, //
                11, 2, 0xEF, 0xBE, 0xAD, 0xDE, 0, 0, 0, 0, 4, 0, 0, 0,
            ],
            &[],
        ),
    );
    put(
        "layout_node",
        "truncated",
        &node(layout::class::TEXT_VIEW, &[(layout::attr::TEXT, 1)], 0)[..8],
    );
}

/// The `tls_verify` input layout: `u64` clock, `u16`-prefixed host, an
/// entry count, `u16`-prefixed DER entries with the last one taking the
/// rest.
fn tls_verify_seed(clock: u64, host: &str, ders: &[&[u8]]) -> Vec<u8> {
    let mut v = clock.to_le_bytes().to_vec();
    v.extend_from_slice(&(host.len() as u16).to_le_bytes());
    v.extend_from_slice(host.as_bytes());
    v.push(ders.len() as u8);
    for (i, d) in ders.iter().enumerate() {
        if i + 1 < ders.len() {
            v.extend_from_slice(&(d.len() as u16).to_le_bytes());
        }
        v.extend_from_slice(d);
    }
    v
}

/// A `std::net::TcpStream` that keeps everything it reads.
struct Recording {
    stream: std::net::TcpStream,
    log: std::sync::Arc<std::sync::Mutex<Vec<u8>>>,
}

impl pd_tls::embedded_io::ErrorType for Recording {
    type Error = pd_tls::embedded_io::ErrorKind;
}

impl pd_tls::embedded_io::Read for Recording {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize, Self::Error> {
        let n = self
            .stream
            .read(buf)
            .map_err(|_| pd_tls::embedded_io::ErrorKind::Other)?;
        self.log.lock().unwrap().extend_from_slice(&buf[..n]);
        Ok(n)
    }
}

impl pd_tls::embedded_io::Write for Recording {
    fn write(&mut self, buf: &[u8]) -> Result<usize, Self::Error> {
        self.stream
            .write(buf)
            .map_err(|_| pd_tls::embedded_io::ErrorKind::Other)
    }
    fn flush(&mut self) -> Result<(), Self::Error> {
        Ok(())
    }
}

/// Capture a server's half of a TLS 1.3 handshake from a local server:
///
///     openssl req -x509 -newkey ec -pkeyopt ec_paramgen_curve:P-256 \
///         -nodes -subj /CN=example.com -keyout k.pem -out c.pem
///     openssl s_server -accept 44330 -tls1_3 -key k.pem -cert c.pem -www
///     PICODROID_TLS_SEED_ADDR=127.0.0.1:44330 \
///         cargo test -p fuzz-targets --test seeds -- --ignored capture_tls_seed
#[test]
#[ignore]
fn capture_tls_seed() {
    let addr = std::env::var("PICODROID_TLS_SEED_ADDR").expect("PICODROID_TLS_SEED_ADDR");
    let stream = std::net::TcpStream::connect(&addr).unwrap();
    stream
        .set_read_timeout(Some(std::time::Duration::from_secs(5)))
        .unwrap();
    let log = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let socket = Recording {
        stream,
        log: log.clone(),
    };
    let mut session = pd_tls::TlsSession::new(socket).unwrap();
    // The self-signed certificate fails verification after the server's
    // whole flight has been read, which is all the seed needs.
    let _ = session.open::<pd_tls::NoClock>("example.com", [7; 32]);
    drop(session);
    let rx = log.lock().unwrap().clone();
    assert!(rx.len() > 100, "captured only {} bytes", rx.len());
    // No read-size prefix: one byte saying "zero sizes", then the stream.
    put("tls_server", "openssl-p256", &[&[0u8][..], &rx].concat());
}
