// SPDX-License-Identifier: GPL-3.0-only
//! Deep validation of a link table against its class bytes.

use crate::classfile::{derive, survey, walk_cp, CpView, Layout, Sink};
use crate::error::LinkError;
use crate::layout::{Link, HEADER_WORDS};

impl CpView for Link<'_> {
    fn cp_count(&self) -> usize {
        Link::cp_count(self)
    }
    fn tag(&self, i: usize) -> Option<u8> {
        self.cp_tag(i)
    }
    fn offset(&self, i: usize) -> Option<usize> {
        self.cp_offset(i)
    }
}

/// Compares each derived word with the table's.
struct CheckSink<'a> {
    words: &'a [u16],
    puts: usize,
}

impl Sink for CheckSink<'_> {
    fn put(&mut self, word: usize, value: u16) -> Result<(), LinkError> {
        match self.words.get(word) {
            Some(&w) if w == value => {
                self.puts += 1;
                Ok(())
            }
            Some(_) => Err(LinkError::WordMismatch { word: word as u16 }),
            None => Err(LinkError::BadOffset { word: word as u16 }),
        }
    }
}

impl Link<'_> {
    /// Check that this table is exactly what [`crate::build::link_class`]
    /// derives from `class`: the constant-pool words are compared against
    /// an independent walk of the pool, then every other word against the
    /// same derivation the builder ran. No allocation; O(class bytes +
    /// table words). Run once per class when a set is built, embedded or
    /// installed — the runtime then trusts the table.
    pub fn validate(&self, class: &[u8]) -> Result<(), LinkError> {
        if self.class_len() != class.len() {
            return Err(LinkError::WordMismatch { word: 2 });
        }
        let header_cp_count = self.cp_count();
        let (cp_count, pos_after_cp) = walk_cp(class, |i, tag, off| {
            if self.cp_tag(i) != Some(tag) {
                return Err(LinkError::WordMismatch {
                    word: (HEADER_WORDS + header_cp_count + i / 2) as u16,
                });
            }
            if self.cp_offset(i) != Some(off) {
                return Err(LinkError::WordMismatch {
                    word: (HEADER_WORDS + i) as u16,
                });
            }
            Ok(())
        })?;
        if cp_count != header_cp_count {
            return Err(LinkError::WordMismatch { word: 3 });
        }
        let counts = survey(class, self, cp_count, pos_after_cp)?;
        let layout = Layout::new(&counts)?;
        if layout.total_words != self.words().len() {
            return Err(LinkError::BadHeader);
        }
        let mut sink = CheckSink {
            words: self.words(),
            puts: 0,
        };
        derive(class, self, &counts, &layout, &mut sink)?;
        if sink.puts != layout.total_words {
            return Err(LinkError::Internal);
        }
        Ok(())
    }
}
