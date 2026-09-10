// SPDX-License-Identifier: GPL-3.0-only
//! A small class-file assembler for hand-built test classes: a constant pool
//! built entry by entry (1-based indices, as the JVM sees them), then a
//! class or interface with any number of methods.
use alloc::vec;
use alloc::vec::Vec;

pub(super) const ACC_INTERFACE: u16 = 0x0601; // PUBLIC | INTERFACE | ABSTRACT

/// `(max_stack, code, exception table rows)` of the single static `m()I`
/// that [`Asm::finish`] emits.
pub(super) type StaticMain<'a> = (u16, &'a [u8], &'a [[u16; 4]]);

pub(super) struct Asm {
    cp: Vec<Vec<u8>>,
    /// Declared instance fields as `(name_utf8, desc_utf8)` CP indices.
    fields: Vec<(u16, u16)>,
}

/// One method to emit. An empty `code` emits no `Code` attribute — an
/// abstract or native declaration.
pub(super) struct Method<'a> {
    pub access: u16,
    pub name: &'a str,
    pub desc: &'a str,
    pub max_stack: u16,
    pub max_locals: u16,
    pub code: &'a [u8],
    /// Exception table rows as `[start, end, handler, catch_type]`.
    pub exc: &'a [[u16; 4]],
}

impl Asm {
    pub(super) fn new() -> Self {
        Self {
            cp: Vec::new(),
            fields: Vec::new(),
        }
    }

    /// Declare an instance field on the class being built.
    pub(super) fn field(&mut self, name: &str, desc: &str) {
        let n = self.utf8(name);
        let d = self.utf8(desc);
        self.fields.push((n, d));
    }

    fn push(&mut self, e: Vec<u8>) -> u16 {
        self.cp.push(e);
        self.cp.len() as u16
    }

    pub(super) fn utf8(&mut self, s: &str) -> u16 {
        let mut e = vec![0x01];
        e.extend_from_slice(&(s.len() as u16).to_be_bytes());
        e.extend_from_slice(s.as_bytes());
        self.push(e)
    }

    pub(super) fn class(&mut self, name: &str) -> u16 {
        let u = self.utf8(name);
        self.push(vec![0x07, (u >> 8) as u8, u as u8])
    }

    pub(super) fn string(&mut self, s: &str) -> u16 {
        let u = self.utf8(s);
        self.push(vec![0x08, (u >> 8) as u8, u as u8])
    }

    fn name_and_type(&mut self, name: &str, desc: &str) -> u16 {
        let n = self.utf8(name);
        let d = self.utf8(desc);
        self.push(vec![0x0C, (n >> 8) as u8, n as u8, (d >> 8) as u8, d as u8])
    }

    /// Methodref (tag 10) or InterfaceMethodref (tag 11).
    pub(super) fn methodref(&mut self, tag: u8, class: u16, name: &str, desc: &str) -> u16 {
        let nat = self.name_and_type(name, desc);
        self.push(vec![
            tag,
            (class >> 8) as u8,
            class as u8,
            (nat >> 8) as u8,
            nat as u8,
        ])
    }

    /// MethodHandle (tag 15): `kind` is the reference kind (6 = invokeStatic).
    pub(super) fn method_handle(&mut self, kind: u8, r: u16) -> u16 {
        self.push(vec![0x0F, kind, (r >> 8) as u8, r as u8])
    }

    /// MethodType (tag 16).
    pub(super) fn method_type(&mut self, desc: &str) -> u16 {
        let u = self.utf8(desc);
        self.push(vec![0x10, (u >> 8) as u8, u as u8])
    }

    /// InvokeDynamic (tag 18) for bootstrap entry `bsm` and `name`/`desc`.
    pub(super) fn invoke_dynamic(&mut self, bsm: u16, name: &str, desc: &str) -> u16 {
        let nat = self.name_and_type(name, desc);
        self.push(vec![
            0x12,
            (bsm >> 8) as u8,
            bsm as u8,
            (nat >> 8) as u8,
            nat as u8,
        ])
    }

    /// Fieldref (tag 9).
    pub(super) fn fieldref(&mut self, class: u16, name: &str, desc: &str) -> u16 {
        let nat = self.name_and_type(name, desc);
        self.push(vec![
            0x09,
            (class >> 8) as u8,
            class as u8,
            (nat >> 8) as u8,
            nat as u8,
        ])
    }

    /// Emit the class file. `method` is `(max_stack, code, exception table
    /// rows)` for a single static `m()I` (`max_locals` 1), or `None` for a
    /// method-less class or interface.
    pub(super) fn finish(
        &mut self,
        access: u16,
        this: u16,
        sup: u16,
        ifaces: &[u16],
        method: Option<StaticMain<'_>>,
    ) -> &'static [u8] {
        match method {
            Some((max_stack, code, exc)) => self.finish_methods(
                access,
                this,
                sup,
                ifaces,
                &[Method {
                    access: 0x0008,
                    name: "m",
                    desc: "()I",
                    max_stack,
                    max_locals: 1,
                    code,
                    exc,
                }],
            ),
            None => self.finish_methods(access, this, sup, ifaces, &[]),
        }
    }

    /// Emit the class file with the given methods.
    pub(super) fn finish_methods(
        &mut self,
        access: u16,
        this: u16,
        sup: u16,
        ifaces: &[u16],
        methods: &[Method<'_>],
    ) -> &'static [u8] {
        self.finish_full(access, this, sup, ifaces, methods, &[])
    }

    /// Emit the class file with the given methods and `BootstrapMethods`
    /// entries, each `(bootstrap MethodHandle, arguments)`.
    pub(super) fn finish_full(
        &mut self,
        access: u16,
        this: u16,
        sup: u16,
        ifaces: &[u16],
        methods: &[Method<'_>],
        bootstraps: &[(u16, &[u16])],
    ) -> &'static [u8] {
        // Constant-pool entries for every method first: the pool is emitted
        // before the method table.
        let names: Vec<(u16, u16)> = methods
            .iter()
            .map(|m| (self.utf8(m.name), self.utf8(m.desc)))
            .collect();
        let code_name = if methods.iter().any(|m| !m.code.is_empty()) {
            self.utf8("Code")
        } else {
            0
        };
        let bsm_name = if bootstraps.is_empty() {
            0
        } else {
            self.utf8("BootstrapMethods")
        };
        let mut out = vec![0xCA, 0xFE, 0xBA, 0xBE, 0x00, 0x00, 0x00, 0x34];
        out.extend_from_slice(&(self.cp.len() as u16 + 1).to_be_bytes());
        for e in &self.cp {
            out.extend_from_slice(e);
        }
        out.extend_from_slice(&access.to_be_bytes());
        out.extend_from_slice(&this.to_be_bytes());
        out.extend_from_slice(&sup.to_be_bytes());
        out.extend_from_slice(&(ifaces.len() as u16).to_be_bytes());
        for i in ifaces {
            out.extend_from_slice(&i.to_be_bytes());
        }
        out.extend_from_slice(&(self.fields.len() as u16).to_be_bytes());
        for &(n, d) in &self.fields {
            out.extend_from_slice(&[0x00, 0x00]); // access
            out.extend_from_slice(&n.to_be_bytes());
            out.extend_from_slice(&d.to_be_bytes());
            out.extend_from_slice(&[0x00, 0x00]); // attrs
        }
        out.extend_from_slice(&(methods.len() as u16).to_be_bytes());
        for (m, (n, d)) in methods.iter().zip(names) {
            out.extend_from_slice(&m.access.to_be_bytes());
            out.extend_from_slice(&n.to_be_bytes());
            out.extend_from_slice(&d.to_be_bytes());
            if m.code.is_empty() {
                out.extend_from_slice(&[0x00, 0x00]); // no attributes
                continue;
            }
            out.extend_from_slice(&[0x00, 0x01]); // attrs
            out.extend_from_slice(&code_name.to_be_bytes());
            let attr_len = 2 + 2 + 4 + m.code.len() + 2 + 8 * m.exc.len() + 2;
            out.extend_from_slice(&(attr_len as u32).to_be_bytes());
            out.extend_from_slice(&m.max_stack.to_be_bytes());
            out.extend_from_slice(&m.max_locals.to_be_bytes());
            out.extend_from_slice(&(m.code.len() as u32).to_be_bytes());
            out.extend_from_slice(m.code);
            out.extend_from_slice(&(m.exc.len() as u16).to_be_bytes());
            for row in m.exc {
                for v in row {
                    out.extend_from_slice(&v.to_be_bytes());
                }
            }
            out.extend_from_slice(&[0x00, 0x00]); // code attrs
        }
        if bootstraps.is_empty() {
            out.extend_from_slice(&[0x00, 0x00]); // class attrs
        } else {
            out.extend_from_slice(&[0x00, 0x01]);
            out.extend_from_slice(&bsm_name.to_be_bytes());
            let len = 2 + bootstraps
                .iter()
                .map(|(_, a)| 4 + 2 * a.len())
                .sum::<usize>();
            out.extend_from_slice(&(len as u32).to_be_bytes());
            out.extend_from_slice(&(bootstraps.len() as u16).to_be_bytes());
            for (handle, args) in bootstraps {
                out.extend_from_slice(&handle.to_be_bytes());
                out.extend_from_slice(&(args.len() as u16).to_be_bytes());
                for a in *args {
                    out.extend_from_slice(&a.to_be_bytes());
                }
            }
        }
        alloc::boxed::Box::leak(out.into_boxed_slice())
    }
}
