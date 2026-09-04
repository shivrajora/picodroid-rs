// SPDX-License-Identifier: GPL-3.0-only
//! Pins the cyw43 port's config invariants (docs/networking-followups-2026-08.md
//! "Validation environment"). Host-only text scan, included from `main.rs`
//! under `cfg(test)`.

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    fn port(name: &str) -> String {
        let p = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("src/hal/rp/port")
            .join(name);
        std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("{}: {e}", p.display()))
    }

    /// Shortening the ioctl timeout below the driver's 500 ms default breaks
    /// CLM finalization and every join fails NONET.
    #[test]
    fn ioctl_timeout_is_never_shortened() {
        let h = port("cyw43_configport.h");
        for l in h.lines().map(str::trim) {
            if let Some(rest) = l.strip_prefix("#define CYW43_IOCTL_TIMEOUT_US") {
                let v: u64 = rest
                    .trim()
                    .trim_matches(|c| c == '(' || c == ')')
                    .parse()
                    .expect("numeric CYW43_IOCTL_TIMEOUT_US");
                assert!(v >= 500_000, "CYW43_IOCTL_TIMEOUT_US {v} < 500000");
            }
        }
    }

    /// The family's part of the IP config exists and does not smuggle shared
    /// policy back in.
    #[test]
    fn family_ip_config_holds_only_family_choices() {
        let h = port("FreeRTOSIPConfig_family.h");
        assert!(h.contains("#define ipconfigIP_TASK_AFFINITY"));
        for shared_only in [
            "ipconfigBUFFER_PADDING",
            "ipconfigPACKET_FILLER_SIZE",
            "ipconfigINCLUDE_FULL_INET_ADDR",
            "ipconfigUSE_IPv",
        ] {
            assert!(
                !h.contains(shared_only),
                "{shared_only} is shared policy (picodroid-core/net-freertos-tcp)"
            );
        }
        assert!(
            !PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("src/hal/rp/port/FreeRTOSIPConfig.h")
                .exists(),
            "FreeRTOSIPConfig.h is shared; the family ships FreeRTOSIPConfig_family.h only"
        );
    }
}
