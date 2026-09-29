// SPDX-License-Identifier: GPL-3.0-only
//! Method descriptors (JVMS §4.3.3).

/// Number of parameters in a method descriptor — one per value whatever
/// its width (the interpreter's operand stack holds one entry per
/// parameter; a `long` is not two slots there). `None` for anything that
/// is not a well-formed descriptor, or has more than 255 parameters (JVMS
/// §4.3.3 caps them at 255 including `this`).
///
/// ```
/// use class_link::count_args;
/// assert_eq!(count_args(b"()V"), Some(0));
/// assert_eq!(count_args(b"(IJ[[Ljava/lang/String;D)V"), Some(4));
/// assert_eq!(count_args(b"(I"), None);
/// ```
pub fn count_args(descriptor: &[u8]) -> Option<u8> {
    if descriptor.first() != Some(&b'(') {
        return None;
    }
    let mut i = 1;
    let mut n: u32 = 0;
    loop {
        match *descriptor.get(i)? {
            b')' => break,
            b'L' => {
                i = skip_object(descriptor, i)?;
                n += 1;
            }
            b'[' => {
                while *descriptor.get(i)? == b'[' {
                    i += 1;
                }
                match *descriptor.get(i)? {
                    b'L' => i = skip_object(descriptor, i)?,
                    b'B' | b'C' | b'D' | b'F' | b'I' | b'J' | b'S' | b'Z' => i += 1,
                    _ => return None,
                }
                n += 1;
            }
            b'B' | b'C' | b'D' | b'F' | b'I' | b'J' | b'S' | b'Z' => {
                i += 1;
                n += 1;
            }
            _ => return None,
        }
    }
    // A return descriptor must follow the `)`.
    if i + 1 >= descriptor.len() {
        return None;
    }
    u8::try_from(n).ok()
}

/// Past an `Lname;` starting at `i` (which holds the `L`).
fn skip_object(d: &[u8], mut i: usize) -> Option<usize> {
    i += 1;
    while *d.get(i)? != b';' {
        i += 1;
    }
    Some(i + 1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counts_every_kind_once() {
        assert_eq!(count_args(b"()V"), Some(0));
        assert_eq!(count_args(b"(I)I"), Some(1));
        assert_eq!(count_args(b"(JD)V"), Some(2));
        assert_eq!(count_args(b"(Ljava/lang/String;I)V"), Some(2));
        assert_eq!(count_args(b"([I[[Ljava/lang/Object;)Z"), Some(2));
        assert_eq!(count_args(b"(BCSZFDIJ)Ljava/lang/Object;"), Some(8));
    }

    #[test]
    fn rejects_malformed() {
        assert_eq!(count_args(b""), None);
        assert_eq!(count_args(b"I"), None);
        assert_eq!(count_args(b"(I"), None);
        assert_eq!(count_args(b"(I)"), None);
        assert_eq!(count_args(b"(Ljava/lang/String)V"), None);
        assert_eq!(count_args(b"([)V"), None);
        assert_eq!(count_args(b"(Q)V"), None);
    }

    #[test]
    fn caps_at_255() {
        let mut d = [b'I'; 258];
        d[0] = b'(';
        d[256] = b')';
        d[257] = b'V';
        assert_eq!(count_args(&d), Some(255));
        let mut d = [b'I'; 259];
        d[0] = b'(';
        d[257] = b')';
        d[258] = b'V';
        assert_eq!(count_args(&d), None);
    }
}
