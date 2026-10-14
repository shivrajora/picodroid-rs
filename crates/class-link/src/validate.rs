// SPDX-License-Identifier: GPL-3.0-only
//! Deep validation of a link table against its class bytes.

use crate::classfile::{derive, survey, walk_cp, CpView, Layout, Sink, Utf8Form, TAG_UTF8};
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
    fn pool(&self) -> &[u8] {
        Link::pool(self)
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

    fn put_external(&mut self, word: usize, _placeholder: u16) -> Result<(), LinkError> {
        if word >= self.words.len() {
            return Err(LinkError::BadOffset { word: word as u16 });
        }
        self.puts += 1;
        Ok(())
    }
}

impl Link<'_> {
    /// Check that this table is exactly what the section builder derives
    /// from the pooled `class`: the constant-pool words are compared
    /// against an independent walk of the pool — a `Utf8`'s word is its
    /// ref, which must name a pool entry — then every other word against
    /// the same derivation the builder ran, reading the `Utf8`s through
    /// those refs. No allocation; O(class bytes + table words). Run once
    /// per class when a set is built, embedded or installed — the runtime
    /// then trusts the table. The String descriptors' literal ids and the
    /// pool words are the section's to check
    /// ([`crate::ClassSection::validate`]).
    pub fn validate(&self, class: &[u8]) -> Result<(), LinkError> {
        if self.class_len() != class.len() {
            return Err(LinkError::WordMismatch { word: 2 });
        }
        let header_cp_count = self.cp_count();
        let (cp_count, pos_after_cp) = walk_cp(class, Utf8Form::Pooled, |i, tag, off| {
            if self.cp_tag(i) != Some(tag) {
                return Err(LinkError::WordMismatch {
                    word: (HEADER_WORDS + header_cp_count + i / 2) as u16,
                });
            }
            if tag == TAG_UTF8 {
                let r = self.cp_word(i).ok_or(LinkError::Internal)?;
                if self.utf8_ref(r as usize).is_none() {
                    return Err(LinkError::BadOffset {
                        word: (HEADER_WORDS + i) as u16,
                    });
                }
                return Ok(());
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
