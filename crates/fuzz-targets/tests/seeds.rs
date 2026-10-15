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
use papk_format::{AlignedBuf, AssetSpec, EntryPoint, ManifestSpec, Papk, PapkBuilder};
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

    // ── TLS ───────────────────────────────────────────────────────────────
    for entry in std::fs::read_dir(root().join("crates/pd-tls/roots")).unwrap() {
        let path = entry.unwrap().path();
        let stem = path.file_stem().unwrap().to_string_lossy().into_owned();
        put("der", &stem, &std::fs::read(&path).unwrap());
    }
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
