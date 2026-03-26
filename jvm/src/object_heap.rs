use crate::types::Value;
use alloc::vec::Vec;

#[allow(dead_code)]
pub struct JvmObject {
    pub class_name: &'static str,
    pub fields: Vec<Value>,
}

pub struct ObjectHeap {
    objects: Vec<Option<JvmObject>>,
    sb_buf: Vec<u8>,
}

impl ObjectHeap {
    pub const fn new() -> Self {
        Self {
            objects: Vec::new(),
            sb_buf: Vec::new(),
        }
    }
}

impl Default for ObjectHeap {
    fn default() -> Self {
        Self::new()
    }
}

impl ObjectHeap {
    /// Allocate a new object of the given class, returning its heap index.
    /// For `java/lang/StringBuilder`, reuses an existing slot if one exists —
    /// the JVM's single shared `sb_buf` makes all StringBuilders equivalent.
    /// Reuses a None slot (freed by GC) before growing the backing Vec.
    pub fn alloc(&mut self, class_name: &'static str) -> Option<u16> {
        if class_name == "java/lang/StringBuilder" {
            if let Some(idx) = self
                .objects
                .iter()
                .position(|o| matches!(o, Some(obj) if obj.class_name == "java/lang/StringBuilder"))
            {
                return Some(idx as u16);
            }
        }
        // Reuse a None slot if available (after GC), otherwise grow
        if let Some(idx) = self
            .objects
            .iter()
            .position(|o: &Option<JvmObject>| o.is_none())
        {
            self.objects[idx] = Some(JvmObject {
                class_name,
                fields: Vec::new(),
            });
            return Some(idx as u16);
        }
        let idx = self.objects.len() as u16;
        self.objects.push(Some(JvmObject {
            class_name,
            fields: Vec::new(),
        }));
        Some(idx)
    }

    pub fn get_field(&self, idx: u16, field: usize) -> Option<Value> {
        self.objects
            .get(idx as usize)?
            .as_ref()?
            .fields
            .get(field)
            .copied()
    }

    pub fn set_field(&mut self, idx: u16, field: usize, v: Value) -> Option<()> {
        let obj = self.objects.get_mut(idx as usize)?.as_mut()?;
        while obj.fields.len() <= field {
            obj.fields.push(Value::Null);
        }
        obj.fields[field] = v;
        Some(())
    }

    #[allow(dead_code)]
    pub fn class_name(&self, idx: u16) -> Option<&'static str> {
        Some(self.objects.get(idx as usize)?.as_ref()?.class_name)
    }

    /// Clear the shared StringBuilder buffer.
    pub fn sb_clear(&mut self) {
        self.sb_buf.clear();
    }

    /// Append bytes to the shared StringBuilder buffer.
    pub fn sb_append_bytes(&mut self, bytes: &[u8]) {
        self.sb_buf.extend_from_slice(bytes);
    }

    /// Append an integer (decimal) to the shared StringBuilder buffer.
    pub fn sb_append_int(&mut self, n: i32) {
        let mut tmp = [0u8; 12];
        let s = int_to_decimal_buf(n, &mut tmp);
        self.sb_append_bytes(s);
    }

    /// Append a long (decimal) to the shared StringBuilder buffer.
    pub fn sb_append_long(&mut self, n: i64) {
        let mut tmp = [0u8; 21];
        let s = long_to_decimal_buf(n, &mut tmp);
        self.sb_append_bytes(s);
    }

    /// Append a float to the shared StringBuilder buffer.
    /// Formats as `[-]integer.fraction` with up to 6 significant decimal digits.
    pub fn sb_append_float(&mut self, f: f32) {
        let mut tmp = [0u8; 32];
        let s = float_to_str_buf(f, &mut tmp);
        self.sb_append_bytes(s);
    }

    /// Return the current length of the StringBuilder buffer.
    pub fn sb_len(&self) -> usize {
        self.sb_buf.len()
    }

    /// Return the byte at `idx` in the StringBuilder buffer, or `None` if out of bounds.
    pub fn sb_char_at(&self, idx: usize) -> Option<u8> {
        self.sb_buf.get(idx).copied()
    }

    /// Return the current StringBuilder buffer contents as a byte slice.
    pub fn sb_contents_slice(&self) -> &[u8] {
        &self.sb_buf
    }

    /// Return the current StringBuilder buffer contents as a raw pointer and length.
    ///
    /// # Safety
    /// The returned pointer is valid until the next call to `sb_append_bytes`,
    /// `sb_append_int`, or `sb_clear`, any of which may reallocate `sb_buf`.
    pub fn sb_contents(&self) -> (*const u8, usize) {
        (self.sb_buf.as_ptr(), self.sb_buf.len())
    }
}

/// Format `n` as a decimal ASCII string into `buf`.  Returns the filled slice.
pub fn long_to_decimal_buf(mut n: i64, buf: &mut [u8; 21]) -> &[u8] {
    if n == 0 {
        buf[0] = b'0';
        return &buf[..1];
    }
    let neg = n < 0;
    if neg {
        n = n.wrapping_neg();
    }
    let mut i = 21usize;
    while n > 0 {
        i -= 1;
        buf[i] = b'0' + (n % 10) as u8;
        n /= 10;
    }
    if neg {
        i -= 1;
        buf[i] = b'-';
    }
    &buf[i..]
}

/// Format `n` as a decimal ASCII string into `buf`.  Returns the filled slice.
pub fn int_to_decimal_buf(mut n: i32, buf: &mut [u8; 12]) -> &[u8] {
    if n == 0 {
        buf[0] = b'0';
        return &buf[..1];
    }
    let neg = n < 0;
    if neg {
        n = n.wrapping_neg();
    }
    let mut i = 12usize;
    while n > 0 {
        i -= 1;
        buf[i] = b'0' + (n % 10) as u8;
        n /= 10;
    }
    if neg {
        i -= 1;
        buf[i] = b'-';
    }
    &buf[i..]
}

/// Format `f` as a decimal ASCII string into `buf`.
/// Produces `[-]integer.fraction` with up to 6 significant decimal digits.
/// Special values: "NaN", "Infinity", "-Infinity".
pub fn float_to_str_buf(f: f32, buf: &mut [u8; 32]) -> &[u8] {
    if f.is_nan() {
        let s = b"NaN";
        buf[..s.len()].copy_from_slice(s);
        return &buf[..s.len()];
    }
    if f.is_infinite() {
        if f > 0.0 {
            let s = b"Infinity";
            buf[..s.len()].copy_from_slice(s);
            return &buf[..s.len()];
        } else {
            let s = b"-Infinity";
            buf[..s.len()].copy_from_slice(s);
            return &buf[..s.len()];
        }
    }
    let neg = f < 0.0;
    let f = if neg { -f } else { f };

    // Integer and fractional parts
    let int_part = f as u32;
    let frac = f - int_part as f32;

    let mut pos = 0usize;
    if neg {
        buf[pos] = b'-';
        pos += 1;
    }

    // Write integer part
    let mut ibuf = [0u8; 12];
    let istr = int_to_decimal_buf(int_part as i32, &mut ibuf);
    buf[pos..pos + istr.len()].copy_from_slice(istr);
    pos += istr.len();

    // Write up to 6 fractional digits, trimming trailing zeros
    buf[pos] = b'.';
    pos += 1;

    let mut frac_val = (frac * 1_000_000.0 + 0.5) as u32; // round to 6 places
                                                          // Trim trailing zeros
    let mut digits = 6usize;
    while digits > 1 && frac_val % 10 == 0 {
        frac_val /= 10;
        digits -= 1;
    }
    // Write digits (right to left, then reverse)
    let frac_start = pos;
    let mut fv = frac_val;
    for _ in 0..digits {
        buf[pos] = b'0' + (fv % 10) as u8;
        fv /= 10;
        pos += 1;
    }
    buf[frac_start..pos].reverse();

    &buf[..pos]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::Value;

    #[test]
    fn alloc_returns_sequential_indices() {
        let mut heap = ObjectHeap::new();
        assert_eq!(heap.alloc("A"), Some(0));
        assert_eq!(heap.alloc("B"), Some(1));
        assert_eq!(heap.alloc("C"), Some(2));
    }

    #[test]
    fn alloc_beyond_old_capacity_succeeds() {
        let mut heap = ObjectHeap::new();
        for i in 0..64 {
            assert!(heap.alloc(if i % 2 == 0 { "X" } else { "Y" }).is_some());
        }
    }

    #[test]
    fn get_field_nonexistent_field_returns_none() {
        let mut heap = ObjectHeap::new();
        heap.alloc("A");
        assert_eq!(heap.get_field(0, 0), None);
    }

    #[test]
    fn set_and_get_field_round_trip() {
        let mut heap = ObjectHeap::new();
        heap.alloc("A");
        heap.set_field(0, 0, Value::Int(42));
        assert_eq!(heap.get_field(0, 0), Some(Value::Int(42)));
    }

    #[test]
    fn set_field_fills_gaps_with_null() {
        let mut heap = ObjectHeap::new();
        heap.alloc("A");
        heap.set_field(0, 2, Value::Int(5));
        assert_eq!(heap.get_field(0, 0), Some(Value::Null));
        assert_eq!(heap.get_field(0, 1), Some(Value::Null));
        assert_eq!(heap.get_field(0, 2), Some(Value::Int(5)));
    }

    #[test]
    fn class_name_returns_correct_name() {
        let mut heap = ObjectHeap::new();
        heap.alloc("MyClass");
        assert_eq!(heap.class_name(0), Some("MyClass"));
    }

    #[test]
    fn class_name_invalid_index_returns_none() {
        let heap = ObjectHeap::new();
        assert_eq!(heap.class_name(99), None);
    }

    #[test]
    fn get_field_invalid_object_returns_none() {
        let heap = ObjectHeap::new();
        assert_eq!(heap.get_field(99, 0), None);
    }

    #[test]
    fn string_builder_reuses_slot() {
        let mut heap = ObjectHeap::new();
        let idx1 = heap.alloc("java/lang/StringBuilder");
        let idx2 = heap.alloc("java/lang/StringBuilder");
        assert!(idx1.is_some());
        assert_eq!(idx1, idx2);
    }

    #[test]
    fn sb_append_bytes_and_contents() {
        let mut heap = ObjectHeap::new();
        heap.sb_append_bytes(b"hello");
        assert_eq!(heap.sb_contents_slice(), b"hello");
        assert_eq!(heap.sb_len(), 5);
    }

    #[test]
    fn sb_clear_resets_length() {
        let mut heap = ObjectHeap::new();
        heap.sb_append_bytes(b"hello");
        heap.sb_clear();
        assert_eq!(heap.sb_len(), 0);
    }

    #[test]
    fn sb_char_at() {
        let mut heap = ObjectHeap::new();
        heap.sb_append_bytes(b"abc");
        assert_eq!(heap.sb_char_at(0), Some(b'a'));
        assert_eq!(heap.sb_char_at(2), Some(b'c'));
        assert_eq!(heap.sb_char_at(3), None);
    }

    #[test]
    fn sb_append_int_zero() {
        let mut heap = ObjectHeap::new();
        heap.sb_append_int(0);
        assert_eq!(heap.sb_contents_slice(), b"0");
    }

    #[test]
    fn sb_append_int_positive() {
        let mut heap = ObjectHeap::new();
        heap.sb_append_int(12345);
        assert_eq!(heap.sb_contents_slice(), b"12345");
    }

    #[test]
    fn sb_append_int_negative() {
        let mut heap = ObjectHeap::new();
        heap.sb_append_int(-42);
        assert_eq!(heap.sb_contents_slice(), b"-42");
    }

    #[test]
    fn sb_append_no_truncation() {
        let mut heap = ObjectHeap::new();
        let long_str = [b'x'; 70];
        heap.sb_append_bytes(&long_str);
        assert_eq!(heap.sb_len(), 70);
    }

    #[test]
    fn sb_append_multiple_then_clear() {
        let mut heap = ObjectHeap::new();
        heap.sb_append_bytes(b"foo");
        heap.sb_append_int(7);
        heap.sb_clear();
        heap.sb_append_bytes(b"bar");
        assert_eq!(heap.sb_contents_slice(), b"bar");
    }

    #[test]
    fn gc_slot_reuse() {
        let mut heap = ObjectHeap::new();
        assert_eq!(heap.alloc("A"), Some(0));
        assert_eq!(heap.alloc("B"), Some(1));
        // Simulate GC freeing slot 0
        heap.objects[0] = None;
        // Next alloc should reuse slot 0
        assert_eq!(heap.alloc("C"), Some(0));
        // Slot 1 still intact
        assert_eq!(heap.class_name(1), Some("B"));
    }

    #[test]
    fn float_to_str_special() {
        let mut buf = [0u8; 32];
        assert_eq!(float_to_str_buf(f32::NAN, &mut buf), b"NaN");
        assert_eq!(float_to_str_buf(f32::INFINITY, &mut buf), b"Infinity");
        assert_eq!(float_to_str_buf(f32::NEG_INFINITY, &mut buf), b"-Infinity");
    }

    #[test]
    fn float_to_str_zero() {
        let mut buf = [0u8; 32];
        let s = float_to_str_buf(0.0, &mut buf);
        assert_eq!(s, b"0.0");
    }

    #[test]
    fn float_to_str_integer() {
        let mut buf = [0u8; 32];
        let s = float_to_str_buf(42.0, &mut buf);
        // 42.0 → "42.0"
        assert_eq!(s, b"42.0");
    }

    #[test]
    fn float_to_str_negative() {
        let mut buf = [0u8; 32];
        let s = float_to_str_buf(-3.14, &mut buf);
        // Should start with "-3."
        assert!(s.starts_with(b"-3."));
    }
}
