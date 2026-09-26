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
                // A cached site was inserted after its class initialised,
                // so a hit answers both questions at once.
                let value = match self
                    .class_objects
                    .resolve
                    .static_index(class_name, field_name)
                {
                    Some(idx) => self.statics.get_by_index(idx),
                    None => {
                        // JVMS §5.4.3.2: the field lives on the class that
                        // declares it, which may be a superclass or a
                        // superinterface of the one the `Fieldref` names; a
                        // field no class file declares reads `Null`.
                        #[cfg(feature = "parity-metrics")]
                        let t0 = self.handler.clock_nanos();
                        let resolved =
                            helpers::resolve_static_field(self.classes, class_name, field_name);
                        #[cfg(feature = "parity-metrics")]
                        crate::parity::count_resolve_time(
                            self.handler.clock_nanos().saturating_sub(t0),
                        );
                        let Some((dci, fi)) = resolved else {
                            frame.push(Value::Null)?;
                            return Ok(());
                        };
                        // JVMS §5.5: `getstatic` initialises the declaring
                        // class (and, through it, its superclasses).
                        #[cfg(feature = "parity-metrics")]
                        let t0 = self.handler.clock_nanos();
                        let pending = self.ensure_class_initialized_at(dci)?;
                        #[cfg(feature = "parity-metrics")]
                        crate::parity::count_clinit_time(
                            self.handler.clock_nanos().saturating_sub(t0),
                        );
                        if pending {
                            frame.pc = frame.inst_pc;
                            return Ok(());
                        }
                        let idx = self
                            .statics
                            .slot(dci, fi)
                            .ok_or(JvmError::InvalidBytecode)?;
                        self.class_objects
                            .resolve
                            .insert_static(class_name, field_name, idx);
                        self.statics.get_by_index(idx)
                    }
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
                match self
                    .class_objects
                    .resolve
                    .static_index(class_name, field_name)
                {
                    Some(idx) => {
                        let value = frame.pop()?;
                        self.statics.set_by_index(idx, value);
                    }
                    None => {
                        #[cfg(feature = "parity-metrics")]
                        let t0 = self.handler.clock_nanos();
                        let resolved =
                            helpers::resolve_static_field(self.classes, class_name, field_name);
                        #[cfg(feature = "parity-metrics")]
                        crate::parity::count_resolve_time(
                            self.handler.clock_nanos().saturating_sub(t0),
                        );
                        let Some((dci, fi)) = resolved else {
                            // No class file declares it: nothing can read
                            // it back, so the value is dropped.
                            frame.pop()?;
                            return Ok(());
                        };
                        #[cfg(feature = "parity-metrics")]
                        let t0 = self.handler.clock_nanos();
                        let pending = self.ensure_class_initialized_at(dci)?;
                        #[cfg(feature = "parity-metrics")]
                        crate::parity::count_clinit_time(
                            self.handler.clock_nanos().saturating_sub(t0),
                        );
                        if pending {
                            frame.pc = frame.inst_pc;
                            return Ok(());
                        }
                        let value = frame.pop()?;
                        let idx = self
                            .statics
                            .slot(dci, fi)
                            .ok_or(JvmError::InvalidBytecode)?;
                        self.class_objects
                            .resolve
                            .insert_static(class_name, field_name, idx);
                        self.statics.set_by_index(idx, value);
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
                        let (declared_class, field_name_bytes, _desc) =
                            cf.cp_fieldref(cp_idx).ok_or(JvmError::InvalidBytecode)?;
                        let obj_class = self
                            .objects
                            .class_name(idx)
                            .ok_or(JvmError::InvalidReference)?;
                        #[cfg(feature = "parity-metrics")]
                        let t0 = self.handler.clock_nanos();
                        let slot = helpers::field_slot_cached(
                            &mut self.class_objects.resolve,
                            self.classes,
                            obj_class,
                            declared_class,
                            field_name_bytes,
                        )
                        .ok_or(JvmError::InvalidReference)?;
                        #[cfg(feature = "parity-metrics")]
                        crate::parity::count_resolve_time(
                            self.handler.clock_nanos().saturating_sub(t0),
                        );
                        let v = self.objects.get_field(idx, slot).unwrap_or(Value::Null);
                        frame.push(v)?;
                    }
                    Value::Null => return Err(self.null_pointer_exception()),
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
                        let (declared_class, field_name_bytes, _desc) =
                            cf.cp_fieldref(cp_idx).ok_or(JvmError::InvalidBytecode)?;
                        let obj_class = self
                            .objects
                            .class_name(idx)
                            .ok_or(JvmError::InvalidReference)?;
                        #[cfg(feature = "parity-metrics")]
                        let t0 = self.handler.clock_nanos();
                        let slot = helpers::field_slot_cached(
                            &mut self.class_objects.resolve,
                            self.classes,
                            obj_class,
                            declared_class,
                            field_name_bytes,
                        )
                        .ok_or(JvmError::InvalidReference)?;
                        #[cfg(feature = "parity-metrics")]
                        crate::parity::count_resolve_time(
                            self.handler.clock_nanos().saturating_sub(t0),
                        );
                        self.objects
                            .set_field(idx, slot, value)
                            .ok_or(JvmError::InvalidReference)?;
                    }
                    Value::Null => return Err(self.null_pointer_exception()),
                    _ => return Err(JvmError::InvalidReference),
                }
            }

            _ => return Err(JvmError::UnsupportedOpcode(opcode)),
        }
        Ok(())
    }
}
