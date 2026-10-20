// SPDX-License-Identifier: GPL-3.0-only
//! A scripted byte pipe for tests and the fuzz harness: reads replay a
//! `Vec<u8>` (zeros past its end, `None` from the timed-out read), writes
//! are recorded.

use alloc::vec::Vec;

use crate::PdbTransport;

pub struct VecPipe {
    pub rx: Vec<u8>,
    pub pos: usize,
    pub tx: Vec<u8>,
    /// Blocking and timed-out reads made so far, past-the-end ones included.
    pub reads: usize,
}

impl VecPipe {
    pub fn new(rx: Vec<u8>) -> Self {
        Self {
            rx,
            pos: 0,
            tx: Vec::new(),
            reads: 0,
        }
    }

    /// Whether every scripted byte has been read.
    pub fn exhausted(&self) -> bool {
        self.pos >= self.rx.len()
    }
}

impl PdbTransport for VecPipe {
    fn init(&mut self) {}
    fn read_byte(&mut self) -> u8 {
        self.reads += 1;
        let b = self.rx.get(self.pos).copied().unwrap_or(0);
        self.pos += 1;
        b
    }
    fn read_byte_timeout(&mut self) -> Option<u8> {
        self.reads += 1;
        self.rx.get(self.pos).copied().inspect(|_| self.pos += 1)
    }
    fn write_bytes(&mut self, data: &[u8]) {
        self.tx.extend_from_slice(data)
    }
    fn drain_tx(&mut self) {}
}
