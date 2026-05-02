// SPDX-License-Identifier: GPL-3.0-only
use super::{helpers, Executor};
use crate::{
    frame::Frame,
    native::NativeMethodHandler,
    types::{JvmError, Value},
};

impl<'a, H: NativeMethodHandler> Executor<'a, H> {
    pub(super) fn op_fields(
        &mut self,
        opcode: u8,
        code: &[u8],
        frame: &mut Frame,
    ) -> Result<(), JvmError> {
        match opcode {
            // getstatic — look up in StaticFieldStore; unset fields read as Null
            0xb2 => {
                let cp_idx = u16::from_be_bytes([code[frame.pc], code[frame.pc + 1]]);
                frame.pc += 2;
                let cf = &self.classes[frame.class_idx];
                let (class_name, field_name, _desc) =
                    cf.cp_fieldref(cp_idx).ok_or(JvmError::InvalidBytecode)?;
                if self.ensure_class_initialized(class_name)? {
                    frame.pc = frame.inst_pc;
                    return Ok(());
                }
                let cn_ptr = class_name.as_ptr();
                let fn_ptr = field_name.as_ptr();
                let value = 'lookup: {
                    for &(cp, fp, idx) in self.static_field_cache.iter() {
                        if cp == cn_ptr && fp == fn_ptr {
                            break 'lookup self.statics.get_by_index(idx);
                        }
                    }
                    let value = self.statics.get(class_name, field_name);
                    if let Some(idx) = self.statics.find_index(class_name, field_name) {
                        self.static_field_cache.push((cn_ptr, fn_ptr, idx));
                    }
                    value
                };
                frame.push(value)?;
            }

            // putstatic — store value into StaticFieldStore
            0xb3 => {
                let cp_idx = u16::from_be_bytes([code[frame.pc], code[frame.pc + 1]]);
                frame.pc += 2;
                let cf = &self.classes[frame.class_idx];
                let (class_name, field_name, _desc) =
                    cf.cp_fieldref(cp_idx).ok_or(JvmError::InvalidBytecode)?;
                if self.ensure_class_initialized(class_name)? {
                    frame.pc = frame.inst_pc;
                    return Ok(());
                }
                let value = frame.pop()?;
                let cn_ptr = class_name.as_ptr();
                let fn_ptr = field_name.as_ptr();
                let mut found = false;
                for &(cp, fp, idx) in self.static_field_cache.iter() {
                    if cp == cn_ptr && fp == fn_ptr {
                        self.statics.set_by_index(idx, value);
                        found = true;
                        break;
                    }
                }
                if !found {
                    self.statics
                        .set(class_name, field_name, value)
                        .ok_or(JvmError::StackOverflow)?;
                    if let Some(idx) = self.statics.find_index(class_name, field_name) {
                        self.static_field_cache.push((cn_ptr, fn_ptr, idx));
                    }
                }
            }

            // getfield — objectref → value
            0xb4 => {
                let cp_idx = u16::from_be_bytes([code[frame.pc], code[frame.pc + 1]]);
                frame.pc += 2;
                let obj_ref = frame.pop()?;
                match obj_ref {
                    Value::ObjectRef(idx) => {
                        let cf = &self.classes[frame.class_idx];
                        let (_class, field_name_bytes, _desc) =
                            cf.cp_fieldref(cp_idx).ok_or(JvmError::InvalidBytecode)?;
                        let obj_class = self
                            .objects
                            .class_name(idx)
                            .ok_or(JvmError::InvalidReference)?;
                        let slot = helpers::field_slot_cached(
                            &mut self.field_cache,
                            self.classes,
                            obj_class,
                            field_name_bytes,
                        )
                        .ok_or(JvmError::InvalidReference)?;
                        let v = self.objects.get_field(idx, slot).unwrap_or(Value::Null);
                        frame.push(v)?;
                    }
                    _ => return Err(JvmError::InvalidReference),
                }
            }

            // putfield — objectref, value →
            0xb5 => {
                let cp_idx = u16::from_be_bytes([code[frame.pc], code[frame.pc + 1]]);
                frame.pc += 2;
                let value = frame.pop()?;
                let obj_ref = frame.pop()?;
                match obj_ref {
                    Value::ObjectRef(idx) => {
                        let cf = &self.classes[frame.class_idx];
                        let (_class, field_name_bytes, _desc) =
                            cf.cp_fieldref(cp_idx).ok_or(JvmError::InvalidBytecode)?;
                        let obj_class = self
                            .objects
                            .class_name(idx)
                            .ok_or(JvmError::InvalidReference)?;
                        let slot = helpers::field_slot_cached(
                            &mut self.field_cache,
                            self.classes,
                            obj_class,
                            field_name_bytes,
                        )
                        .ok_or(JvmError::InvalidReference)?;
                        self.objects
                            .set_field(idx, slot, value)
                            .ok_or(JvmError::InvalidReference)?;
                    }
                    _ => return Err(JvmError::InvalidReference),
                }
            }

            _ => return Err(JvmError::UnsupportedOpcode(opcode)),
        }
        Ok(())
    }
}
