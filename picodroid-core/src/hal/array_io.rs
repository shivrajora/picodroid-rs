// SPDX-License-Identifier: GPL-3.0-only
//! Java `byte[]` ↔ slice staging for the bus natives, shared by every family.
//!
//! `picodroid.pio.I2cDevice` and `SpiDevice` hand the natives a Java array by
//! heap index. Every family used to copy the same forty lines per bus to
//! turn that into a slice, call its bus, and copy the result back. The
//! `HalI2c` and `HalSpi` default methods do it once, here, over the family's
//! slice functions; a family implements `write_slice`/`read_slice` and
//! `write_raw`/`transfer_raw` and gets the Java entry points for free.
//!
//! Every transfer is staged through a fixed stack buffer. A Java array can be
//! larger than that, so the contract each helper keeps is: `-1` when `len`
//! exceeds the cap, `0` for an empty transfer, else what the bus said. The
//! cap is the trait's `JAVA_XFER_MAX`, at most [`STAGING_CAP`].

use pico_jvm::array_heap::ArrayHeap;

/// Largest Java-array transfer the default shims stage on the stack, in
/// bytes. Both RP buses cap at 64 today; a family whose bus takes more
/// overrides the trait method rather than this.
pub const STAGING_CAP: usize = 64;

/// The effective cap: what the trait asked for, but never more than the
/// buffer can hold.
fn cap(max: usize) -> usize {
    max.min(STAGING_CAP)
}

/// Copy `len` elements of Java `byte[] idx` into the front of `buf`.
fn stage_out<'b>(arrays: &ArrayHeap, idx: u16, len: usize, buf: &'b mut [u8]) -> &'b [u8] {
    for (i, slot) in buf.iter_mut().enumerate().take(len) {
        *slot = arrays.load(idx, i).unwrap_or(0) as u8;
    }
    &buf[..len]
}

/// Copy `src` into Java `byte[] idx`, from element 0.
fn stage_in(arrays: &mut ArrayHeap, idx: u16, src: &[u8]) {
    for (i, &b) in src.iter().enumerate() {
        arrays.store(idx, i, b as i32);
    }
}

/// `HalI2c::write` over `write_slice`: `-1` when `len` exceeds the cap, else
/// the slice call's result.
pub fn i2c_write(
    max: usize,
    arrays: &ArrayHeap,
    data_idx: u16,
    len: usize,
    write_slice: impl FnOnce(&[u8]) -> i32,
) -> i32 {
    if len > cap(max) {
        return -1;
    }
    let mut buf = [0u8; STAGING_CAP];
    write_slice(stage_out(arrays, data_idx, len, &mut buf))
}

/// `HalI2c::read` over `read_slice`: `0` for an empty read, `-1` when `len`
/// exceeds the cap, else the slice call's result — with the bytes it produced
/// (at most `len`) copied back into the Java array.
pub fn i2c_read(
    max: usize,
    arrays: &mut ArrayHeap,
    buf_idx: u16,
    len: usize,
    read_slice: impl FnOnce(&mut [u8]) -> i32,
) -> i32 {
    if len == 0 {
        return 0;
    }
    if len > cap(max) {
        return -1;
    }
    let mut buf = [0u8; STAGING_CAP];
    let result = read_slice(&mut buf[..len]);
    if result > 0 {
        let n = (result as usize).min(len);
        stage_in(arrays, buf_idx, &buf[..n]);
    }
    result
}

/// `HalSpi::transfer` over `transfer_raw`: full duplex, `len` bytes out of
/// `tx_idx` and into `rx_idx`. Returns `len`, `0` for an empty transfer, or
/// `-1` when `len` exceeds the cap.
pub fn spi_transfer(
    max: usize,
    arrays: &mut ArrayHeap,
    tx_idx: u16,
    rx_idx: u16,
    len: usize,
    transfer_raw: impl FnOnce(&[u8], &mut [u8]),
) -> i32 {
    if len == 0 {
        return 0;
    }
    if len > cap(max) {
        return -1;
    }
    let mut tx = [0u8; STAGING_CAP];
    let mut rx = [0u8; STAGING_CAP];
    transfer_raw(stage_out(arrays, tx_idx, len, &mut tx), &mut rx[..len]);
    stage_in(arrays, rx_idx, &rx[..len]);
    len as i32
}

/// `HalSpi::write` over `write_raw`: same shape, nothing read back.
pub fn spi_write(
    max: usize,
    arrays: &ArrayHeap,
    data_idx: u16,
    len: usize,
    write_raw: impl FnOnce(&[u8]),
) -> i32 {
    if len == 0 {
        return 0;
    }
    if len > cap(max) {
        return -1;
    }
    let mut buf = [0u8; STAGING_CAP];
    write_raw(stage_out(arrays, data_idx, len, &mut buf));
    len as i32
}

#[cfg(test)]
mod tests {
    use super::*;
    use pico_jvm::array_heap::ATYPE_BYTE;

    fn byte_array(arrays: &mut ArrayHeap, bytes: &[u8]) -> u16 {
        let idx = arrays.alloc(ATYPE_BYTE, bytes.len() as u16).expect("alloc");
        for (i, &b) in bytes.iter().enumerate() {
            arrays.store(idx, i, b as i32);
        }
        idx
    }

    fn read_back(arrays: &ArrayHeap, idx: u16, len: usize) -> alloc::vec::Vec<u8> {
        (0..len)
            .map(|i| arrays.load(idx, i).unwrap() as u8)
            .collect()
    }

    #[test]
    fn i2c_write_hands_the_bus_exactly_the_java_bytes() {
        let mut arrays = ArrayHeap::new();
        let idx = byte_array(&mut arrays, &[1, 2, 3, 250]);
        let mut seen = alloc::vec::Vec::new();
        let r = i2c_write(64, &arrays, idx, 4, |d| {
            seen.extend_from_slice(d);
            d.len() as i32
        });
        assert_eq!(r, 4);
        assert_eq!(seen, [1, 2, 3, 250]);
    }

    #[test]
    fn i2c_write_stages_only_len_bytes_of_a_longer_array() {
        let mut arrays = ArrayHeap::new();
        let idx = byte_array(&mut arrays, &[9, 8, 7, 6, 5]);
        let mut seen = alloc::vec::Vec::new();
        i2c_write(64, &arrays, idx, 2, |d| {
            seen.extend_from_slice(d);
            0
        });
        assert_eq!(seen, [9, 8]);
    }

    #[test]
    fn i2c_read_copies_back_what_the_bus_produced_and_no_more() {
        let mut arrays = ArrayHeap::new();
        let idx = byte_array(&mut arrays, &[0xAA; 6]);
        // The bus fills three of the four requested bytes and says so.
        let r = i2c_read(64, &mut arrays, idx, 4, |b| {
            b[0] = 1;
            b[1] = 2;
            b[2] = 3;
            3
        });
        assert_eq!(r, 3);
        // Three written, the fourth untouched, the rest untouched.
        assert_eq!(read_back(&arrays, idx, 6), [1, 2, 3, 0xAA, 0xAA, 0xAA]);
    }

    #[test]
    fn i2c_read_of_nothing_is_zero_without_touching_the_bus() {
        let mut arrays = ArrayHeap::new();
        let idx = byte_array(&mut arrays, &[7]);
        let r = i2c_read(64, &mut arrays, idx, 0, |_| panic!("bus touched"));
        assert_eq!(r, 0);
    }

    #[test]
    fn i2c_read_error_leaves_the_array_alone() {
        let mut arrays = ArrayHeap::new();
        let idx = byte_array(&mut arrays, &[5, 5]);
        let r = i2c_read(64, &mut arrays, idx, 2, |b| {
            b[0] = 99;
            -1
        });
        assert_eq!(r, -1);
        assert_eq!(read_back(&arrays, idx, 2), [5, 5]);
    }

    #[test]
    fn the_cap_is_inclusive_and_one_past_it_is_refused() {
        let mut arrays = ArrayHeap::new();
        let idx = byte_array(&mut arrays, &[0; 65]);
        // 64 is fine (the cap), 65 is not: an off-by-one on either side of
        // the comparison fails one of these.
        assert_eq!(i2c_write(64, &arrays, idx, 64, |d| d.len() as i32), 64);
        assert_eq!(
            i2c_write(64, &arrays, idx, 65, |_| panic!("bus touched")),
            -1
        );
        assert_eq!(spi_write(64, &arrays, idx, 64, |_| {}), 64);
        assert_eq!(
            spi_write(64, &arrays, idx, 65, |_| panic!("bus touched")),
            -1
        );
        assert_eq!(
            i2c_read(64, &mut arrays, idx, 65, |_| panic!("bus touched")),
            -1
        );
        assert_eq!(
            spi_transfer(64, &mut arrays, idx, idx, 65, |_, _| panic!("bus touched")),
            -1
        );
    }

    #[test]
    fn a_trait_cap_above_the_buffer_is_clamped_to_it() {
        let mut arrays = ArrayHeap::new();
        let idx = byte_array(&mut arrays, &[0; 65]);
        // A family that claims 128 still stages through a 64-byte buffer.
        assert_eq!(
            i2c_write(128, &arrays, idx, 65, |_| panic!("bus touched")),
            -1
        );
    }

    #[test]
    fn spi_transfer_round_trips_through_a_loopback_bus() {
        let mut arrays = ArrayHeap::new();
        let tx = byte_array(&mut arrays, &[10, 20, 30]);
        let rx = byte_array(&mut arrays, &[0, 0, 0, 0xEE]);
        let r = spi_transfer(64, &mut arrays, tx, rx, 3, |t, r| r.copy_from_slice(t));
        assert_eq!(r, 3);
        assert_eq!(read_back(&arrays, rx, 4), [10, 20, 30, 0xEE]);
    }

    #[test]
    fn spi_write_reports_the_length_it_sent() {
        let mut arrays = ArrayHeap::new();
        let idx = byte_array(&mut arrays, &[1, 2]);
        let mut sent = 0;
        assert_eq!(spi_write(64, &arrays, idx, 2, |d| sent = d.len()), 2);
        assert_eq!(sent, 2);
        assert_eq!(spi_write(64, &arrays, idx, 0, |_| panic!("bus touched")), 0);
    }
}
