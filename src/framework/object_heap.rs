use crate::framework::types::Value;
use heapless::Vec;

#[allow(dead_code)]
pub struct JvmObject {
    pub class_name: &'static str,
    pub fields: Vec<Value, 8>,
}

pub struct ObjectHeap {
    objects: Vec<JvmObject, 16>,
}

impl ObjectHeap {
    pub const fn new() -> Self {
        Self {
            objects: Vec::new(),
        }
    }

    /// Allocate a new object of the given class, returning its heap index.
    pub fn alloc(&mut self, class_name: &'static str) -> Option<u16> {
        let idx = self.objects.len() as u16;
        self.objects
            .push(JvmObject {
                class_name,
                fields: Vec::new(),
            })
            .ok()?;
        Some(idx)
    }

    pub fn get_field(&self, idx: u16, field: usize) -> Option<Value> {
        self.objects.get(idx as usize)?.fields.get(field).copied()
    }

    pub fn set_field(&mut self, idx: u16, field: usize, v: Value) -> Option<()> {
        let obj = self.objects.get_mut(idx as usize)?;
        while obj.fields.len() <= field {
            obj.fields.push(Value::Null).ok()?;
        }
        obj.fields[field] = v;
        Some(())
    }

    #[allow(dead_code)]
    pub fn class_name(&self, idx: u16) -> Option<&'static str> {
        Some(self.objects.get(idx as usize)?.class_name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::framework::types::Value;

    #[test]
    fn alloc_returns_sequential_indices() {
        let mut heap = ObjectHeap::new();
        assert_eq!(heap.alloc("A"), Some(0));
        assert_eq!(heap.alloc("B"), Some(1));
        assert_eq!(heap.alloc("C"), Some(2));
    }

    #[test]
    fn alloc_full_returns_none() {
        let mut heap = ObjectHeap::new();
        for _ in 0..16 {
            assert!(heap.alloc("X").is_some());
        }
        assert_eq!(heap.alloc("X"), None);
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
}
