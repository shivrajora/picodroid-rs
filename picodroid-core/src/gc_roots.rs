// SPDX-License-Identifier: GPL-3.0-only
//! GC root provider registry.
//!
//! # Why this exists
//!
//! The JVM cannot see objects that only native code holds. A View kept alive
//! solely by a native listener map, a bound Service, the Display singleton —
//! each must be reported to the collector or it is swept while still
//! reachable from LVGL, and the next callback resolves a live widget to a
//! dead reference. This project has shipped that bug more than once; it
//! presents as input dying a few seconds in, or `NoSuchMethod` after the
//! first GC.
//!
//! Until now `PicodroidNativeHandler::gc_visit_roots` enumerated every
//! provider by hand, reaching across the whole framework from one function.
//! That is fine in a single crate and impossible across two: the extraction
//! moves those providers a few files at a time, and any commit that left the
//! enumeration on one side of the boundary while a provider sat on the other
//! would compile, run, and silently collect live objects.
//!
//! So providers register themselves, and the handler just visits whatever is
//! registered. A provider moving between crates changes which file calls
//! [`register_object_refs`], not whether the union is complete. This is also
//! audit item P2-17, and it breaks the `native_handler` → `graphics`
//! dependency cycle as a side effect.
//!
//! # Registration discipline
//!
//! Register once, at the top of app boot, before any class is loaded and
//! therefore before any GC can run. Registration is not idempotent — calling
//! it twice would visit every root twice, which is wasteful but not unsound.
//!
//! # Implementation notes
//!
//! Fixed-capacity array, no allocation, no atomics: registration happens on
//! one task during boot, and `visit_all` runs inside the collector with the
//! world already stopped. This also keeps it usable on thumbv6m, which has
//! no compare-and-swap.

use pico_jvm::types::Value;

/// Maximum registered providers. Bump when [`register`] starts panicking —
/// there is no dynamic growth by design (no allocation inside GC).
pub const MAX_PROVIDERS: usize = 32;

/// A provider that reports `Value`s directly.
pub type ValueProvider = fn(&mut dyn FnMut(Value));

/// A provider that reports raw Java object references.
///
/// Most native listener maps store `obj_ref: u16` rather than a full
/// `Value`, so they get their own shape instead of forcing every one of them
/// through an adapter closure.
pub type ObjectRefProvider = fn(&mut dyn FnMut(u16));

#[derive(Clone, Copy)]
enum Provider {
    Values(ValueProvider),
    ObjectRefs(ObjectRefProvider),
}

struct Registry {
    providers: [Option<Provider>; MAX_PROVIDERS],
    count: usize,
}

// SAFETY: single-threaded access by construction. Registration happens on the
// boot task before any other task exists; `visit_all` runs from the collector
// with the world stopped. This is the same single-core contract the object
// heap itself relies on (see app.rs's SharedHeapCell).
struct RegistryCell(core::cell::UnsafeCell<Registry>);
unsafe impl Sync for RegistryCell {}

static REGISTRY: RegistryCell = RegistryCell(core::cell::UnsafeCell::new(Registry {
    providers: [None; MAX_PROVIDERS],
    count: 0,
}));

fn registry() -> &'static mut Registry {
    unsafe { &mut *REGISTRY.0.get() }
}

fn push(p: Provider) {
    let reg = registry();
    assert!(
        reg.count < MAX_PROVIDERS,
        "GC root provider registry full (MAX_PROVIDERS = {MAX_PROVIDERS}); \
         raise the cap rather than dropping a provider — an unregistered \
         provider is a silent premature-collection bug"
    );
    reg.providers[reg.count] = Some(p);
    reg.count += 1;
}

/// Register a provider that reports `Value`s.
pub fn register(p: ValueProvider) {
    push(Provider::Values(p));
}

/// Register a provider that reports raw object references.
pub fn register_object_refs(p: ObjectRefProvider) {
    push(Provider::ObjectRefs(p));
}

/// Visit every registered root. Called from the JVM's root scan.
pub fn visit_all(visit: &mut dyn FnMut(Value)) {
    let reg = registry();
    for slot in reg.providers.iter().take(reg.count) {
        match slot {
            Some(Provider::Values(f)) => f(visit),
            Some(Provider::ObjectRefs(f)) => f(&mut |r| visit(Value::ObjectRef(r))),
            None => {}
        }
    }
}

/// How many providers are registered.
///
/// Exists so the platform can assert its expected count: a provider silently
/// dropped during the extraction is invisible at runtime until objects start
/// disappearing, so the count is pinned by a test and bumped deliberately.
pub fn provider_count() -> usize {
    registry().count
}

/// Drop all registrations. Tests only — a running system registers once at
/// boot and never unregisters.
#[cfg(test)]
pub fn reset_for_test() {
    let reg = registry();
    reg.providers = [None; MAX_PROVIDERS];
    reg.count = 0;
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec::Vec;

    fn values_provider(visit: &mut dyn FnMut(Value)) {
        visit(Value::ObjectRef(10));
        visit(Value::ObjectRef(11));
    }

    fn refs_provider(visit: &mut dyn FnMut(u16)) {
        visit(20);
    }

    fn collect() -> Vec<u16> {
        let mut seen = Vec::new();
        visit_all(&mut |v| {
            if let Value::ObjectRef(r) = v {
                seen.push(r);
            }
        });
        seen
    }

    // These share one global registry, so they run as a single test rather
    // than racing each other under the default parallel harness.
    #[test]
    fn registry_visits_both_provider_shapes_in_order() {
        reset_for_test();
        assert_eq!(provider_count(), 0);
        assert!(collect().is_empty(), "empty registry visits nothing");

        register(values_provider);
        register_object_refs(refs_provider);
        assert_eq!(provider_count(), 2);

        // Object-ref providers are lifted to Value::ObjectRef, and providers
        // are visited in registration order.
        assert_eq!(collect(), alloc::vec![10, 11, 20]);

        // Re-visiting is stable: the collector runs this repeatedly.
        assert_eq!(collect(), alloc::vec![10, 11, 20]);

        reset_for_test();
        assert_eq!(provider_count(), 0);
        assert!(collect().is_empty());
    }
}
