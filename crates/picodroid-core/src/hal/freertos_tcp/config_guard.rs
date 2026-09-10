// SPDX-License-Identifier: GPL-3.0-only
//! Pins the invariants of the shared FreeRTOS+TCP glue in
//! `picodroid-core/net-freertos-tcp/` (docs/designs/network-seam-2026-09.md).
//! Host-only text scans, included from `lib.rs` under `cfg(test)`.

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    fn shared(name: &str) -> String {
        let p = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("net-freertos-tcp")
            .join(name);
        std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("{}: {e}", p.display()))
    }

    /// `#define NAME VALUE` lines with all whitespace removed, so a value
    /// comparison does not depend on column alignment.
    fn defines(text: &str) -> Vec<String> {
        text.lines()
            .map(str::trim)
            .filter(|l| l.starts_with("#define"))
            .map(|l| l.split_whitespace().collect::<String>())
            .collect()
    }

    #[test]
    fn shared_ip_config_keeps_the_invariants() {
        let h = shared("FreeRTOSIPConfig.h");
        let d = defines(&h);
        let has = |s: &str| d.iter().any(|l| l == s);
        let defines_name = |n: &str| d.iter().any(|l| l.starts_with(&format!("#define{n}")));

        // No upstream default: unset, every raw-IP URL fails to "resolve".
        assert!(
            has("#defineipconfigINCLUDE_FULL_INET_ADDR(1)"),
            "INCLUDE_FULL_INET_ADDR must be (1)"
        );
        // Padding 8 would put the IPv4/IPv6 discriminator inside the
        // descriptor stamp (see the comment in the header).
        assert!(
            !defines_name("ipconfigBUFFER_PADDING"),
            "never define ipconfigBUFFER_PADDING"
        );
        assert!(
            has("#defineipconfigPACKET_FILLER_SIZE(2)"),
            "PACKET_FILLER_SIZE must be (2)"
        );
        // Core placement is the family's decision (FreeRTOSIPConfig_family.h).
        assert!(
            !defines_name("ipconfigIP_TASK_AFFINITY"),
            "affinity belongs to the family header"
        );
        // The family's part must come first so its choices win the defaults.
        let first_include = h
            .lines()
            .map(str::trim)
            .find(|l| l.starts_with("#include"))
            .expect("an #include");
        assert_eq!(first_include, "#include \"FreeRTOSIPConfig_family.h\"");
        // Priority and stack are overridable defaults, not fixed policy.
        for n in [
            "ipconfigIP_TASK_PRIORITY",
            "ipconfigIP_TASK_STACK_SIZE_WORDS",
        ] {
            assert!(
                h.contains(&format!("#ifndef {n}")),
                "{n} must be #ifndef-wrapped"
            );
        }
    }

    #[test]
    fn shared_glue_binds_the_two_seams_and_names_no_chip() {
        let c = shared("net_init.c");
        for sym in [
            "pxPicodroidNetLink_FillInterfaceDescriptor(",
            "picodroid_port_entropy32(",
            "picodroid_net_ip_event(",
        ] {
            assert!(c.contains(sym), "net_init.c must bind {sym}");
        }
        assert!(
            c.contains("_Static_assert(configTICK_RATE_HZ == 1000"),
            "tick-rate assert"
        );
        let lower = c.to_ascii_lowercase();
        for word in ["cyw43", "0x400b", "rp2350", "rp2040", "trng"] {
            assert!(
                !lower.contains(word),
                "shared net_init.c names a chip: {word}"
            );
        }
        assert!(shared("libc_str.c").contains("strcmp"));
    }
}
