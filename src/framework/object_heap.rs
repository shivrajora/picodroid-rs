use heapless::Vec;
use crate::framework::types::Value;

pub struct JvmObject {
    pub class_name: &'static str,
    pub fields: Vec<Value, 8>,
}

pub struct ObjectHeap {
    objects: Vec<JvmObject, 16>,
}

impl ObjectHeap {
    pub const fn new() -> Self {
        Self { objects: Vec::new() }
    }

    /// Allocate a new object of the given class, returning its heap index.
    pub fn alloc(&mut self, class_name: &'static str) -> Option<u16> {
        let idx = self.objects.len() as u16;
        self.objects
            .push(JvmObject { class_name, fields: Vec::new() })
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

    pub fn class_name(&self, idx: u16) -> Option<&'static str> {
        Some(self.objects.get(idx as usize)?.class_name)
    }
}
