// SPDX-License-Identifier: GPL-3.0-only
//! `pdb logcat` — Android-logcat-style tag/level filtering for picodroid logs.
//!
//! Mode 1 (`--stdin`, no extra features) filters already-decoded text: the
//! simulator's `[Tag] msg` stdout, or piped `probe-rs`/`defmt-print` output
//! (`<LEVEL> Tag: msg`). It does not decode the defmt wire format — pipe a
//! decoder into it:
//!
//!     ./scripts/sim.sh --app foo | pdb logcat --stdin --tag Foo
//!     probe-rs ... | pdb logcat --stdin --level WARN
//!
//! Mode 2 (integrated attach + RTT defmt decode from a firmware ELF) is the
//! `rtt` cargo feature; until it's built in, the pipe above is the path.

use std::io::{self, BufRead, Write};

/// Severity ladder, mirroring `picodroid.util.Log` / defmt. Ordered so a
/// `--level` floor is a simple `>=` comparison.
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub enum Level {
    Verbose = 0,
    Debug = 1,
    Info = 2,
    Warn = 3,
    Error = 4,
}

impl Level {
    /// Parse a `--level` argument (case-insensitive; single letter or word).
    pub fn parse(s: &str) -> Option<Level> {
        match s.to_ascii_uppercase().as_str() {
            "V" | "VERBOSE" | "TRACE" | "TRC" => Some(Level::Verbose),
            "D" | "DEBUG" | "DBG" => Some(Level::Debug),
            "I" | "INFO" => Some(Level::Info),
            "W" | "WARN" | "WARNING" => Some(Level::Warn),
            "E" | "ERROR" | "ERR" => Some(Level::Error),
            _ => None,
        }
    }
}

/// Best-effort level of a decoded line, or `None` when it carries none (the
/// simulator prints every level as `[Tag] msg`). Scans alphabetic tokens for a
/// level word — the defmt/probe-rs prefix is `INFO`/`WARN`/… ahead of the
/// message.
fn line_level(line: &str) -> Option<Level> {
    for tok in line.split(|c: char| !c.is_ascii_alphabetic()) {
        let lvl = match tok.to_ascii_uppercase().as_str() {
            "ERROR" => Some(Level::Error),
            "WARN" | "WARNING" => Some(Level::Warn),
            "INFO" => Some(Level::Info),
            "DEBUG" => Some(Level::Debug),
            "TRACE" | "VERBOSE" => Some(Level::Verbose),
            _ => None,
        };
        if lvl.is_some() {
            return lvl;
        }
    }
    None
}

/// Whether `line` carries `tag` in a tag position: the sim's `[Tag]` form, or
/// the `Tag:` token of a `Tag: msg` defmt line.
fn line_has_tag(line: &str, tag: &str) -> bool {
    if line.contains(&format!("[{tag}]")) {
        return true;
    }
    let needle = format!("{tag}:");
    line.split_whitespace().any(|tok| tok == needle)
}

/// A line passes when it matches the tag (if set) and meets the level floor.
/// Lines with no detectable level pass a `--level` filter — the simulator is
/// level-agnostic, so dropping them would hide all sim output.
fn keep(line: &str, tag: Option<&str>, min_level: Option<Level>) -> bool {
    if let Some(t) = tag {
        if !line_has_tag(line, t) {
            return false;
        }
    }
    if let Some(min) = min_level {
        if let Some(lvl) = line_level(line) {
            if lvl < min {
                return false;
            }
        }
    }
    true
}

/// Read lines from `input`, write the ones that pass the filter to `output`.
fn filter_stream<R: BufRead, W: Write>(
    input: R,
    mut output: W,
    tag: Option<&str>,
    min_level: Option<Level>,
) -> io::Result<()> {
    for line in input.lines() {
        let line = line?;
        if keep(&line, tag, min_level) {
            writeln!(output, "{line}")?;
        }
    }
    Ok(())
}

/// Entry point for `pdb logcat`. Returns the process exit code.
pub fn run(args: &[String]) -> i32 {
    let mut use_stdin = false;
    let mut tag: Option<String> = None;
    let mut min_level: Option<Level> = None;
    let mut elf: Option<String> = None;

    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--stdin" => use_stdin = true,
            "--tag" => {
                i += 1;
                match args.get(i) {
                    Some(t) => tag = Some(t.clone()),
                    None => {
                        eprintln!("error: --tag requires a value");
                        return 1;
                    }
                }
            }
            "--level" => {
                i += 1;
                match args.get(i).and_then(|s| Level::parse(s)) {
                    Some(l) => min_level = Some(l),
                    None => {
                        eprintln!("error: --level requires one of V/D/I/W/E");
                        return 1;
                    }
                }
            }
            "--elf" => {
                i += 1;
                match args.get(i) {
                    Some(p) => elf = Some(p.clone()),
                    None => {
                        eprintln!("error: --elf requires a path");
                        return 1;
                    }
                }
            }
            other => {
                eprintln!("error: unknown logcat argument {other:?}");
                return 1;
            }
        }
        i += 1;
    }

    if elf.is_some() && !use_stdin {
        // Mode 2: integrated attach + RTT defmt decode. Not built into this
        // binary — it needs the `rtt` cargo feature (probe-rs + defmt-decoder).
        // The decode-then-pipe path works today and needs no extra features.
        eprintln!(
            "pdb logcat --elf (live RTT decode) requires the 'rtt' feature, not yet wired.\n\
             Today, pipe a decoder into --stdin instead:\n    \
             probe-rs ... | pdb logcat --stdin --tag <T> --level <L>"
        );
        return 2;
    }

    if !use_stdin {
        eprintln!("error: pdb logcat needs --stdin (filter decoded text on stdin)");
        eprintln!("       e.g. ./scripts/sim.sh --app foo | pdb logcat --stdin --tag Foo");
        return 1;
    }

    let stdin = io::stdin();
    let stdout = io::stdout();
    match filter_stream(stdin.lock(), stdout.lock(), tag.as_deref(), min_level) {
        Ok(()) => 0,
        // A downstream `head`/closed pipe is a normal way to stop a stream.
        Err(e) if e.kind() == io::ErrorKind::BrokenPipe => 0,
        Err(e) => {
            eprintln!("error: {e}");
            1
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn level_parse_and_order() {
        assert_eq!(Level::parse("warn"), Some(Level::Warn));
        assert_eq!(Level::parse("E"), Some(Level::Error));
        assert_eq!(Level::parse("nope"), None);
        assert!(Level::Error > Level::Info);
        assert!(Level::Verbose < Level::Debug);
    }

    #[test]
    fn detect_level_in_decoded_lines() {
        assert_eq!(line_level("INFO  Net: connected"), Some(Level::Info));
        assert_eq!(line_level("[WARN ] Sensor: hot"), Some(Level::Warn));
        // Sim output carries no level.
        assert_eq!(line_level("[Net] connected"), None);
    }

    #[test]
    fn tag_matches_both_formats() {
        assert!(line_has_tag("[Net] connected", "Net"));
        assert!(line_has_tag("INFO  Net: connected", "Net"));
        assert!(!line_has_tag("[Net] connected", "Sensor"));
        // A tag substring of another word must not match.
        assert!(!line_has_tag("[Network] up", "Net"));
    }

    #[test]
    fn keep_combines_tag_and_level() {
        // tag + level both satisfied
        assert!(keep("WARN  Net: drop", Some("Net"), Some(Level::Warn)));
        // right tag, level below floor
        assert!(!keep("INFO  Net: ok", Some("Net"), Some(Level::Warn)));
        // wrong tag
        assert!(!keep("WARN  Sensor: hot", Some("Net"), Some(Level::Warn)));
        // sim line (no level) passes a level floor
        assert!(keep("[Net] ok", Some("Net"), Some(Level::Error)));
    }

    #[test]
    fn filter_stream_selects_lines() {
        let input = "[Net] up\n[Sensor] hot\n[Net] down\n";
        let mut out = Vec::new();
        filter_stream(input.as_bytes(), &mut out, Some("Net"), None).unwrap();
        assert_eq!(String::from_utf8(out).unwrap(), "[Net] up\n[Net] down\n");
    }
}
