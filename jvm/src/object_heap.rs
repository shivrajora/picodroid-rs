use crate::types::Value;
use alloc::vec::Vec;

/// Maximum number of fields stored inline (no heap allocation).
const INLINE_FIELDS: usize = 4;

pub struct JvmObject {
    pub class_name: &'static str,
    field_count: u8,
    inline_fields: [Value; INLINE_FIELDS],
    /// Overflow storage for objects with more than INLINE_FIELDS fields.
    /// `None` when all fields fit inline.
    overflow: Option<Vec<Value>>,
}

impl JvmObject {
    fn new(class_name: &'static str) -> Self {
        Self {
            class_name,
            field_count: 0,
            inline_fields: [Value::Null; INLINE_FIELDS],
            overflow: None,
        }
    }

    fn get_field(&self, field: usize) -> Option<Value> {
        if field >= self.field_count as usize {
            return None;
        }
        if field < INLINE_FIELDS {
            Some(self.inline_fields[field])
        } else {
            self.overflow.as_ref()?.get(field - INLINE_FIELDS).copied()
        }
    }

    fn set_field(&mut self, field: usize, v: Value) {
        // Grow field_count to cover `field`.
        let needed = field + 1;
        if needed > self.field_count as usize {
            // Fill any gap slots with Null.
            if needed > INLINE_FIELDS {
                let ov = self.overflow.get_or_insert_with(Vec::new);
                let ov_needed = needed - INLINE_FIELDS;
                while ov.len() < ov_needed {
                    ov.push(Value::Null);
                }
            }
            self.field_count = needed as u8;
        }
        if field < INLINE_FIELDS {
            self.inline_fields[field] = v;
        } else {
            self.overflow.as_mut().unwrap()[field - INLINE_FIELDS] = v;
        }
    }

    /// Returns (inline_slice, overflow_slice) covering all set fields.
    fn field_slices(&self) -> (&[Value], &[Value]) {
        let inline_count = core::cmp::min(self.field_count as usize, INLINE_FIELDS);
        let inline = &self.inline_fields[..inline_count];
        let overflow = match &self.overflow {
            Some(v) => v.as_slice(),
            None => &[],
        };
        (inline, overflow)
    }
}

/// Metadata for a lambda proxy object created by `invokedynamic`.
pub struct LambdaProxy {
    pub target_class_idx: usize,
    pub target_method_idx: usize,
    pub captures: Vec<Value>,
}

pub struct ObjectHeap {
    objects: Vec<Option<JvmObject>>,
    /// Lowest index that might contain a `None` slot; avoids O(n) scans.
    first_free: usize,
    sb_buf: Vec<u8>,
    list_bufs: Vec<Option<Vec<Value>>>,
    /// Sparse list of lambda proxy metadata, keyed by object index.
    lambda_proxies: Vec<(u16, LambdaProxy)>,
}

impl ObjectHeap {
    pub const fn new() -> Self {
        Self {
            objects: Vec::new(),
            first_free: 0,
            sb_buf: Vec::new(),
            list_bufs: Vec::new(),
            lambda_proxies: Vec::new(),
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
        // Scan from first_free for a None slot; skip already-occupied prefix.
        while self.first_free < self.objects.len() {
            if self.objects[self.first_free].is_none() {
                let idx = self.first_free;
                self.objects[idx] = Some(JvmObject::new(class_name));
                self.first_free = idx + 1;
                return Some(idx as u16);
            }
            self.first_free += 1;
        }
        let idx = self.objects.len() as u16;
        self.objects.push(Some(JvmObject::new(class_name)));
        self.first_free = self.objects.len();
        Some(idx)
    }

    pub fn get_field(&self, idx: u16, field: usize) -> Option<Value> {
        self.objects.get(idx as usize)?.as_ref()?.get_field(field)
    }

    pub fn set_field(&mut self, idx: u16, field: usize, v: Value) -> Option<()> {
        self.objects
            .get_mut(idx as usize)?
            .as_mut()?
            .set_field(field, v);
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

    // ── Lambda proxy support ──────────────────────────────────────────────────

    /// Associate a lambda proxy with an existing heap object.
    pub fn register_lambda(&mut self, obj_idx: u16, proxy: LambdaProxy) {
        self.lambda_proxies.push((obj_idx, proxy));
    }

    /// Look up the lambda proxy metadata for an object, if any.
    pub fn get_lambda(&self, obj_idx: u16) -> Option<&LambdaProxy> {
        self.lambda_proxies
            .iter()
            .find(|(idx, _)| *idx == obj_idx)
            .map(|(_, proxy)| proxy)
    }

    /// Remove the lambda proxy entry for an object (called from GC sweep).
    pub fn free_lambda(&mut self, obj_idx: u16) {
        self.lambda_proxies.retain(|(idx, _)| *idx != obj_idx);
    }

    // ── GC support ────────────────���─────────────────────────────��─────────────

    /// Total number of slots (including freed `None` slots).
    pub fn slot_count(&self) -> usize {
        self.objects.len()
    }

    /// Returns `true` if the slot at `idx` contains a live object.
    pub fn is_live(&self, idx: u16) -> bool {
        self.objects.get(idx as usize).is_some_and(|o| o.is_some())
    }

    /// Free the object at `idx`, setting its slot to `None`.
    pub fn free(&mut self, idx: u16) {
        let i = idx as usize;
        if let Some(slot) = self.objects.get_mut(i) {
            *slot = None;
            if i < self.first_free {
                self.first_free = i;
            }
        }
    }

    /// Return (inline_slice, overflow_slice) covering all fields of the object at `idx`.
    pub fn field_slices(&self, idx: u16) -> (&[Value], &[Value]) {
        self.objects
            .get(idx as usize)
            .and_then(|o| o.as_ref())
            .map(|o| o.field_slices())
            .unwrap_or((&[], &[]))
    }

    /// Return the current StringBuilder buffer contents as a raw pointer and length.
    ///
    /// # Safety
    /// The returned pointer is valid until the next call to `sb_append_bytes`,
    /// `sb_append_int`, or `sb_clear`, any of which may reallocate `sb_buf`.
    pub fn sb_contents(&self) -> (*const u8, usize) {
        (self.sb_buf.as_ptr(), self.sb_buf.len())
    }

    // ── ArrayList / list_bufs ────────────────────────────────────────────────

    /// Allocate a new list buffer, returning its index.
    /// Reuses a `None` slot (freed by GC) before growing the backing Vec.
    pub fn list_alloc(&mut self) -> Option<u16> {
        if let Some(idx) = self.list_bufs.iter().position(|s| s.is_none()) {
            self.list_bufs[idx] = Some(Vec::new());
            return Some(idx as u16);
        }
        let idx = self.list_bufs.len() as u16;
        self.list_bufs.push(Some(Vec::new()));
        Some(idx)
    }

    /// Free a list buffer slot (GC hook). No-op if `idx` is out of range.
    pub fn list_free(&mut self, idx: u16) {
        if let Some(slot) = self.list_bufs.get_mut(idx as usize) {
            *slot = None;
        }
    }

    /// Return the number of elements in the list.
    pub fn list_len(&self, idx: u16) -> usize {
        self.list_bufs
            .get(idx as usize)
            .and_then(|s| s.as_ref())
            .map(|v| v.len())
            .unwrap_or(0)
    }

    /// Return the element at position `i`, or `None` if out of bounds.
    pub fn list_get(&self, idx: u16, i: usize) -> Option<Value> {
        self.list_bufs.get(idx as usize)?.as_ref()?.get(i).copied()
    }

    /// Append `v` to the end of the list.
    pub fn list_add(&mut self, idx: u16, v: Value) {
        if let Some(Some(buf)) = self.list_bufs.get_mut(idx as usize) {
            buf.push(v);
        }
    }

    /// Insert `v` at position `i`, shifting subsequent elements right.
    /// If `i >= len`, appends to the end.
    pub fn list_insert(&mut self, idx: u16, i: usize, v: Value) {
        if let Some(Some(buf)) = self.list_bufs.get_mut(idx as usize) {
            let pos = i.min(buf.len());
            buf.insert(pos, v);
        }
    }

    /// Replace the element at position `i` with `v`, returning the old value.
    /// Returns `None` if `i` is out of bounds.
    pub fn list_set(&mut self, idx: u16, i: usize, v: Value) -> Option<Value> {
        let buf = self.list_bufs.get_mut(idx as usize)?.as_mut()?;
        let old = *buf.get(i)?;
        buf[i] = v;
        Some(old)
    }

    /// Remove and return the element at position `i`.
    /// Returns `None` if `i` is out of bounds.
    pub fn list_remove(&mut self, idx: u16, i: usize) -> Option<Value> {
        let buf = self.list_bufs.get_mut(idx as usize)?.as_mut()?;
        if i < buf.len() {
            Some(buf.remove(i))
        } else {
            None
        }
    }

    /// Remove all elements from the list.
    pub fn list_clear(&mut self, idx: u16) {
        if let Some(Some(buf)) = self.list_bufs.get_mut(idx as usize) {
            buf.clear();
        }
    }

    /// Return an iterator over the list elements.
    pub fn list_iter(&self, idx: u16) -> impl Iterator<Item = Value> + '_ {
        self.list_bufs
            .get(idx as usize)
            .and_then(|s| s.as_ref())
            .map(|v| v.iter().copied())
            .into_iter()
            .flatten()
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
        heap.first_free = 0;
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

    // ── list_bufs tests ──────────────────────────────────────────────────────

    #[test]
    fn list_alloc_returns_sequential_indices() {
        let mut heap = ObjectHeap::new();
        assert_eq!(heap.list_alloc(), Some(0));
        assert_eq!(heap.list_alloc(), Some(1));
        assert_eq!(heap.list_alloc(), Some(2));
    }

    #[test]
    fn list_add_and_get() {
        let mut heap = ObjectHeap::new();
        let idx = heap.list_alloc().unwrap();
        heap.list_add(idx, Value::Int(10));
        heap.list_add(idx, Value::Int(20));
        assert_eq!(heap.list_len(idx), 2);
        assert_eq!(heap.list_get(idx, 0), Some(Value::Int(10)));
        assert_eq!(heap.list_get(idx, 1), Some(Value::Int(20)));
        assert_eq!(heap.list_get(idx, 2), None);
    }

    #[test]
    fn list_set_returns_old_value() {
        let mut heap = ObjectHeap::new();
        let idx = heap.list_alloc().unwrap();
        heap.list_add(idx, Value::Int(1));
        let old = heap.list_set(idx, 0, Value::Int(99));
        assert_eq!(old, Some(Value::Int(1)));
        assert_eq!(heap.list_get(idx, 0), Some(Value::Int(99)));
    }

    #[test]
    fn list_remove_returns_value_and_shifts() {
        let mut heap = ObjectHeap::new();
        let idx = heap.list_alloc().unwrap();
        heap.list_add(idx, Value::Int(1));
        heap.list_add(idx, Value::Int(2));
        heap.list_add(idx, Value::Int(3));
        let removed = heap.list_remove(idx, 1);
        assert_eq!(removed, Some(Value::Int(2)));
        assert_eq!(heap.list_len(idx), 2);
        assert_eq!(heap.list_get(idx, 1), Some(Value::Int(3)));
    }

    #[test]
    fn list_insert_shifts_right() {
        let mut heap = ObjectHeap::new();
        let idx = heap.list_alloc().unwrap();
        heap.list_add(idx, Value::Int(1));
        heap.list_add(idx, Value::Int(3));
        heap.list_insert(idx, 1, Value::Int(2));
        assert_eq!(heap.list_len(idx), 3);
        assert_eq!(heap.list_get(idx, 1), Some(Value::Int(2)));
        assert_eq!(heap.list_get(idx, 2), Some(Value::Int(3)));
    }

    #[test]
    fn list_clear_empties_list() {
        let mut heap = ObjectHeap::new();
        let idx = heap.list_alloc().unwrap();
        heap.list_add(idx, Value::Int(1));
        heap.list_add(idx, Value::Int(2));
        heap.list_clear(idx);
        assert_eq!(heap.list_len(idx), 0);
    }

    #[test]
    fn list_free_slot_is_reused() {
        let mut heap = ObjectHeap::new();
        let idx0 = heap.list_alloc().unwrap();
        let _idx1 = heap.list_alloc().unwrap();
        heap.list_free(idx0);
        let reused = heap.list_alloc().unwrap();
        assert_eq!(reused, idx0);
    }

    #[test]
    fn list_iter_yields_elements() {
        let mut heap = ObjectHeap::new();
        let idx = heap.list_alloc().unwrap();
        heap.list_add(idx, Value::Int(7));
        heap.list_add(idx, Value::Int(8));
        let collected: alloc::vec::Vec<Value> = heap.list_iter(idx).collect();
        assert_eq!(collected, [Value::Int(7), Value::Int(8)]);
    }
}
