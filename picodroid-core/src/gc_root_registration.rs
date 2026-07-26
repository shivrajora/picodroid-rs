// SPDX-License-Identifier: GPL-3.0-only
//! Registers this crate's GC root providers.
//!
//! The counterpart of the platform crate's list. Together the two cover
//! every native holder of Java objects; as the extraction moves modules
//! here, each provider's line moves from that list to this one in the same
//! commit as its file, so the union never changes size.
//!
//! Why a registry at all — and why missing an entry is silent rather than
//! loud — is explained in [`crate::gc_roots`].

/// Providers registered by this crate.
///
/// Rises by exactly the amount the platform crate's `EXPECTED_PROVIDERS`
/// falls whenever a module moves. If only one of the two changes in a
/// commit, a provider was dropped.
pub const EXPECTED_PROVIDERS: usize = 1;

/// Register every root provider owned by this crate.
///
/// Called from the platform's own registration, before the first class load
/// and therefore before any GC can run.
///
/// `cfg(not(test))` to match the modules it names — see
/// [`crate::graphics::lvgl`] for why the LVGL-calling ones are gated.
#[cfg(not(test))]
pub fn register_all() {
    // Animation end-actions hold the Runnable to fire on completion; nothing
    // on the Java side references it for the duration of the animation.
    crate::gc_roots::register_object_refs(
        crate::graphics::lvgl::animations::visit_end_action_roots,
    );
}
