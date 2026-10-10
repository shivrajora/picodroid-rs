// SPDX-License-Identifier: GPL-3.0-only
//! The running app's compiled resources — the RESOURCES section of its PAPK
//! (`papk_format::res`), behind `picodroid.content.res.Resources`.
//!
//! Nothing is copied at load: the registry is one slice into the package,
//! which sits in XIP flash (or the simulated app region) for as long as the
//! app runs. A lookup is offset arithmetic over that slice; only
//! `getString` allocates, and only the Java `String` it returns.

use papk_format::res::{config, ResTable, TYPE_BOOL, TYPE_COLOR, TYPE_DIMEN, TYPE_INTEGER};
use papk_format::Papk;

struct Cell(core::cell::Cell<Option<&'static [u8]>>);
// SAFETY: written by `init_from_papk` / `clear` on the JVM task before and
// after the app runs, read by natives on JVM threads in between — the same
// single-writer discipline as `graphics::assets`.
unsafe impl Sync for Cell {}

static TABLE: Cell = Cell(core::cell::Cell::new(None));

/// How many override blocks a launch keeps, best match first; an app with
/// more matching variant directories than this loses the worst ones.
const MAX_SELECTED: usize = 8;

struct Selection(core::cell::Cell<([u8; MAX_SELECTED], u8)>);
// SAFETY: as `TABLE`: one writer on the JVM task around the app's run.
unsafe impl Sync for Selection {}

/// The override blocks that apply to this run (docs/designs/
/// app-portability-2026-10.md D8), as indices into the table's blocks in
/// Android's precedence, chosen once by [`init_from_papk`].
static SELECTED: Selection = Selection(core::cell::Cell::new(([0; MAX_SELECTED], 0)));

/// The manifest's `android:theme` as an `R.style` id, 0 when the manifest
/// names none (docs/designs/manifest-components-2026-10.md). Read by
/// `Resources.applyManifestTheme` before the first Activity's `onCreate`.
/// An atomic (load/store only, thumbv6m has no RMW) rather than a cell, so
/// it needs no `Sync` promise.
static MANIFEST_THEME: core::sync::atomic::AtomicU32 = core::sync::atomic::AtomicU32::new(0);

/// The manifest theme's `R.style` id, or 0.
pub fn manifest_theme() -> u32 {
    MANIFEST_THEME.load(core::sync::atomic::Ordering::Relaxed)
}

/// Point the registry at `papk`'s RESOURCES section. A package without one
/// (no `res/` tree, or any PAPK below v1.2) leaves it empty, and every
/// lookup then misses.
///
/// The papk is the image the app runs from, mapped for as long as it does
/// (`boot::run_app`'s `apk_static`); [`clear`] drops the slice before the
/// package can change.
pub fn init_from_papk(papk: &Papk<'static>) {
    MANIFEST_THEME.store(
        papk.theme().unwrap_or(0),
        core::sync::atomic::Ordering::Relaxed,
    );
    let section = match papk.resources_section() {
        Ok(Some((_, data))) if ResTable::parse(data).is_ok() => Some(data),
        Ok(None) => None,
        _ => {
            #[cfg(not(feature = "sim"))]
            defmt::error!("[res] RESOURCES section is malformed; ignored");
            #[cfg(feature = "sim")]
            println!("[res] RESOURCES section is malformed; ignored");
            None
        }
    };
    TABLE.0.set(section);
    // Which variant directories apply: the app's window (its design size,
    // else the panel) and whether the board has a touch panel.
    let (w, h) = crate::graphics::lvgl::window::size();
    let cfg = config::Config {
        w_dp: w,
        h_dp: h,
        touch: cfg!(has_touch),
    };
    let mut picked = [0u8; MAX_SELECTED];
    let mut total = 0usize;
    let n = match table() {
        Some(t) => config::best_first(
            t.overrides().map(|o| {
                total += 1;
                o.qualifiers
            }),
            &cfg,
            &mut picked,
        ),
        None => 0,
    };
    SELECTED.0.set((picked, n as u8));
    if total > 0 {
        crate::pd_info!(
            "[res] {}x{}dp {} {}: {} of {} variants apply",
            w,
            h,
            if w > h { "land" } else { "port" },
            if cfg.touch { "finger" } else { "notouch" },
            n,
            total
        );
    }
}

/// Forget the table. Called on app reset, before the package can change.
pub fn clear() {
    TABLE.0.set(None);
    SELECTED.0.set(([0; MAX_SELECTED], 0));
    MANIFEST_THEME.store(0, core::sync::atomic::Ordering::Relaxed);
}

fn table() -> Option<ResTable<'static>> {
    // `init_from_papk` validated the slice it stored.
    ResTable::parse(TABLE.0.get()?).ok()
}

/// A lookup through the selected override blocks, then the base table.
fn resolved<R>(f: impl FnOnce(papk_format::res::Resolved<'static, '_>) -> Option<R>) -> Option<R> {
    let t = table()?;
    let (picked, n) = SELECTED.0.get();
    f(t.with(&picked[..n as usize]))
}

/// The UTF-8 bytes of string resource `id`.
pub fn string(id: i32) -> Option<&'static [u8]> {
    resolved(|r| r.string(id as u32))
}

pub fn color(id: i32) -> Option<i32> {
    resolved(|r| r.value_of(TYPE_COLOR, id as u32)).map(|v| v as i32)
}

/// Pixels.
pub fn dimension(id: i32) -> Option<f32> {
    resolved(|r| r.value_of(TYPE_DIMEN, id as u32)).map(f32::from_bits)
}

pub fn integer(id: i32) -> Option<i32> {
    resolved(|r| r.value_of(TYPE_INTEGER, id as u32)).map(|v| v as i32)
}

pub fn boolean(id: i32) -> Option<bool> {
    resolved(|r| r.value_of(TYPE_BOOL, id as u32)).map(|v| v != 0)
}

/// Word `index` of layout `id`; `None` for a bad id or past the end.
pub fn layout_word(id: i32, index: i32) -> Option<i32> {
    let index = usize::try_from(index).ok()?;
    resolved(|r| r.layout(id as u32))?
        .word(index)
        .map(|w| w as i32)
}

/// Word `index` of style `id`, or the stream's length for `index` -1;
/// `None` for a bad id or past the end.
pub fn style_word(id: i32, index: i32) -> Option<i32> {
    let words = resolved(|r| r.style(id as u32))?;
    if index == -1 {
        return i32::try_from(words.len()).ok();
    }
    words.word(usize::try_from(index).ok()?).map(|w| w as i32)
}

/// The ASSETS entry name behind drawable resource `id`.
pub fn drawable_name(id: i32) -> Option<&'static str> {
    core::str::from_utf8(resolved(|r| r.drawable_name(id as u32))?).ok()
}
