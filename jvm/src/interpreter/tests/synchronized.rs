// SPDX-License-Identifier: GPL-3.0-only
//! `ACC_SYNCHRONIZED` methods: the interpreter takes the receiver's (or, for
//! a static method, the Class object's) monitor when the frame is pushed and
//! releases it on every exit — a return, a caught exception, an uncaught
//! one — reporting each through the handler's `monitor_enter`/`monitor_exit`.
use super::asm::{Asm, Method};
use super::*;
use alloc::vec;
use alloc::vec::Vec;
use crate::types::MonitorKey;

const OBJECT: &str = "java/lang/Object";
const ACC_SYNC: u16 = 0x0020;

fn h(x: u16) -> u8 {
    (x >> 8) as u8
}
fn l(x: u16) -> u8 {
    x as u8
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Ev {
    Enter(MonitorKey),
    Exit(MonitorKey),
}

struct RecordingHandler {
    events: Vec<Ev>,
    refuse_enter: bool,
}

impl NativeMethodHandler for RecordingHandler {
    fn dispatch(
        &mut self,
        _class_name: &str,
        _method_name: &str,
        _ctx: &mut NativeContext<'_>,
    ) -> Option<Result<Option<Value>, JvmError>> {
        None
    }
    fn monitor_enter(&mut self, key: MonitorKey) -> Result<(), JvmError> {
        if self.refuse_enter {
            return Err(JvmError::IllegalMonitorState);
        }
        self.events.push(Ev::Enter(key));
        Ok(())
    }
    fn monitor_exit(&mut self, key: MonitorKey) -> Result<(), JvmError> {
        self.events.push(Ev::Exit(key));
        Ok(())
    }
}

/// Class `S`: `synchronized im()I` → 5; `static synchronized sm()I` → 7;
/// `synchronized thrower()I` throws a `RuntimeException`;
/// `synchronized rec(I)I` recurses down to 0.
fn class_s() -> &'static [u8] {
    let mut a = Asm::new();
    let this = a.class("S");
    let obj = a.class(OBJECT);
    let rec = a.methodref(0x0A, this, "rec", "(I)I");
    let rte = a.class("java/lang/RuntimeException");
    let rte_init = a.methodref(0x0A, rte, "<init>", "()V");
    let im = [0x08, 0xAC]; // iconst_5; ireturn
    let sm = [0x10, 7, 0xAC]; // bipush 7; ireturn
    // new RuntimeException; dup; invokespecial <init>; athrow
    let thrower = [0xBB, h(rte), l(rte), 0x59, 0xB7, h(rte_init), l(rte_init), 0xBF];
    // iload_1; ifeq → iconst_0; aload_0; iload_1; iconst_1; isub;
    // invokevirtual rec; ireturn; iconst_0; ireturn
    let rec_code = [
        0x1B, 0x99, 0x00, 0x0B, 0x2A, 0x1B, 0x04, 0x64, 0xB6, h(rec), l(rec), 0xAC, 0x03, 0xAC,
    ];
    a.finish_methods(
        0x0001,
        this,
        obj,
        &[],
        &[
            Method {
                access: 0x0001 | ACC_SYNC,
                name: "im",
                desc: "()I",
                max_stack: 1,
                max_locals: 1,
                code: &im,
                exc: &[],
            },
            Method {
                access: 0x0009 | ACC_SYNC,
                name: "sm",
                desc: "()I",
                max_stack: 1,
                max_locals: 0,
                code: &sm,
                exc: &[],
            },
            Method {
                access: 0x0001 | ACC_SYNC,
                name: "thrower",
                desc: "()I",
                max_stack: 2,
                max_locals: 1,
                code: &thrower,
                exc: &[],
            },
            Method {
                access: 0x0001 | ACC_SYNC,
                name: "rec",
                desc: "(I)I",
                max_stack: 3,
                max_locals: 2,
                code: &rec_code,
                exc: &[],
            },
        ],
    )
}

/// Class `T` with a static, non-synchronized `m()I` whose body `body`
/// assembles, given the CP index of class `S`.
fn main_class(body: impl FnOnce(&mut Asm, u16) -> (Vec<u8>, Vec<[u16; 4]>)) -> &'static [u8] {
    let mut a = Asm::new();
    let this = a.class("T");
    let obj = a.class(OBJECT);
    let s = a.class("S");
    let (code, exc) = body(&mut a, s);
    a.finish(0x0001, this, obj, &[], Some((3, &code, &exc)))
}

fn run_main(t: &'static [u8], refuse_enter: bool) -> (Result<Option<Value>, JvmError>, Vec<Ev>, ObjectHeap) {
    let classes: Vec<ClassFile> = [class_s(), t]
        .iter()
        .map(|d| ClassFile::parse(d).expect("parse failed"))
        .collect();
    let mut strings = StringTable::new();
    let mut objects = ObjectHeap::new();
    let mut arrays = crate::array_heap::ArrayHeap::new();
    let mut statics = StaticFieldStore::new();
    let mut gc_state = GcState::new();
    let mut class_objects = crate::class_objects::ClassObjectCache::new();
    let mut handler = RecordingHandler {
        events: Vec::new(),
        refuse_enter,
    };
    let r = execute(
        &classes,
        &mut strings,
        &mut objects,
        &mut arrays,
        &mut statics,
        &mut gc_state,
        &mut class_objects,
        &mut handler,
        1,
        0,
        &[],
    );
    (r, handler.events, objects)
}

fn key_of(ev: Ev) -> MonitorKey {
    match ev {
        Ev::Enter(k) | Ev::Exit(k) => k,
    }
}

#[test]
fn instance_and_static_synchronized_methods_lock_receiver_and_class_object() {
    // new S; invokevirtual S.im; invokestatic S.sm; iadd; ireturn
    let t = main_class(|a, s| {
        let im = a.methodref(0x0A, s, "im", "()I");
        let sm = a.methodref(0x0A, s, "sm", "()I");
        (
            vec![0xBB, h(s), l(s), 0xB6, h(im), l(im), 0xB8, h(sm), l(sm), 0x60, 0xAC],
            vec![],
        )
    });
    let (r, ev, objects) = run_main(t, false);
    assert_eq!(r.unwrap(), Some(Value::Int(12)));
    assert_eq!(ev.len(), 4, "{ev:?}");
    let receiver = key_of(ev[0]);
    let class = key_of(ev[2]);
    assert_eq!(ev, vec![Ev::Enter(receiver), Ev::Exit(receiver), Ev::Enter(class), Ev::Exit(class)]);
    assert_ne!(receiver, class);
    let MonitorKey::Object(s) = receiver else { panic!("{receiver:?}") };
    assert_eq!(objects.class_name(s), Some("S"));
    let MonitorKey::Object(c) = class else { panic!("{class:?}") };
    assert_eq!(objects.class_name(c), Some("java/lang/Class"));
}

#[test]
fn a_caught_exception_leaving_a_synchronized_method_releases_its_monitor() {
    // new S; invokevirtual S.thrower; ireturn | handler: pop; iconst_m1; ireturn
    let t = main_class(|a, s| {
        let th = a.methodref(0x0A, s, "thrower", "()I");
        (
            vec![0xBB, h(s), l(s), 0xB6, h(th), l(th), 0xAC, 0x57, 0x02, 0xAC],
            vec![[0, 7, 7, 0]],
        )
    });
    let (r, ev, _) = run_main(t, false);
    assert_eq!(r.unwrap(), Some(Value::Int(-1)));
    assert!(matches!(ev[..], [Ev::Enter(a), Ev::Exit(b)] if a == b), "{ev:?}");
}

#[test]
fn an_uncaught_exception_leaving_a_synchronized_method_releases_its_monitor() {
    let t = main_class(|a, s| {
        let th = a.methodref(0x0A, s, "thrower", "()I");
        (vec![0xBB, h(s), l(s), 0xB6, h(th), l(th), 0xAC], vec![])
    });
    let (r, ev, _) = run_main(t, false);
    assert!(
        matches!(
            r,
            Err(JvmError::UncaughtException {
                exception_class: "java/lang/RuntimeException",
                ..
            })
        ),
        "{r:?}"
    );
    assert!(matches!(ev[..], [Ev::Enter(a), Ev::Exit(b)] if a == b), "{ev:?}");
}

#[test]
fn recursion_through_a_synchronized_method_nests_enter_and_exit() {
    // new S; iconst_2; invokevirtual S.rec(I)I; ireturn
    let t = main_class(|a, s| {
        let rec = a.methodref(0x0A, s, "rec", "(I)I");
        (vec![0xBB, h(s), l(s), 0x05, 0xB6, h(rec), l(rec), 0xAC], vec![])
    });
    let (r, ev, _) = run_main(t, false);
    assert_eq!(r.unwrap(), Some(Value::Int(0)));
    assert_eq!(ev.len(), 6, "{ev:?}");
    let k = key_of(ev[0]);
    let expected: Vec<Ev> = core::iter::repeat(Ev::Enter(k))
        .take(3)
        .chain(core::iter::repeat(Ev::Exit(k)).take(3))
        .collect();
    assert_eq!(ev, expected);
}

#[test]
fn a_refused_monitor_is_the_invoke_error_and_pushes_no_frame() {
    let t = main_class(|a, s| {
        let im = a.methodref(0x0A, s, "im", "()I");
        (vec![0xBB, h(s), l(s), 0xB6, h(im), l(im), 0xAC], vec![])
    });
    let (r, ev, _) = run_main(t, true);
    assert!(matches!(r, Err(JvmError::IllegalMonitorState)), "{r:?}");
    assert!(ev.is_empty(), "no exit without an enter: {ev:?}");
}
