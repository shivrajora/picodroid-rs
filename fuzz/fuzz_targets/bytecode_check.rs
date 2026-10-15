// SPDX-License-Identifier: GPL-3.0-only
#![no_main]

libfuzzer_sys::fuzz_target!(|data: &[u8]| fuzz_targets::class::bytecode_check(data));
