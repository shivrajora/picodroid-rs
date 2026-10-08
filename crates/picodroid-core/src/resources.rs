// SPDX-License-Identifier: GPL-3.0-only
//! The running app's compiled resources — the RESOURCES section of its PAPK
//! (`papk_format::res`), behind `picodroid.content.res.Resources`.
//!
//! Nothing is copied at load: the registry is one slice into the package,
//! which sits in XIP flash (or the simulated app region) for as long as the
//! app runs. A lookup is offset arithmetic over that slice; only
//! `getString` allocates, and only the Java `String` it returns.

use papk_format::res::{ResTable, TYPE_BOOL, TYPE_COLOR, TYPE_DIMEN, TYPE_INTEGER};
use papk_format::Papk;

struct Cell(core::cell::Cell<Option<&'static [u8]>>);
// SAFETY: written by `init_from_papk` / `clear` on the JVM task before and
// after the app runs, read by natives on JVM threads in between — the same
// single-writer discipline as `graphics::assets`.
unsafe impl Sync for Cell {}

static TABLE: Cell = Cell(core::cell::Cell::new(None));

/// Point the registry at `papk`'s RESOURCES section. A package without one
/// (no `res/` tree, or any PAPK below v1.2) leaves it empty, and every
/// lookup then misses.
///
/// The papk is the image the app runs from, mapped for as long as it does
/// (`boot::run_app`'s `apk_static`); [`clear`] drops the slice before the
/// package can change.
pub fn init_from_papk(papk: &Papk<'static>) {
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
}

/// Forget the table. Called on app reset, before the package can change.
pub fn clear() {
    TABLE.0.set(None);
}

fn table() -> Option<ResTable<'static>> {
    // `init_from_papk` validated the slice it stored.
    ResTable::parse(TABLE.0.get()?).ok()
}

/// The UTF-8 bytes of string resource `id`.
pub fn string(id: i32) -> Option<&'static [u8]> {
    table()?.string(id as u32)
}

pub fn color(id: i32) -> Option<i32> {
    table()?.value_of(TYPE_COLOR, id as u32).map(|v| v as i32)
}

/// Pixels.
pub fn dimension(id: i32) -> Option<f32> {
    table()?.value_of(TYPE_DIMEN, id as u32).map(f32::from_bits)
}

pub fn integer(id: i32) -> Option<i32> {
    table()?.value_of(TYPE_INTEGER, id as u32).map(|v| v as i32)
}

pub fn boolean(id: i32) -> Option<bool> {
    table()?.value_of(TYPE_BOOL, id as u32).map(|v| v != 0)
}

/// Word `index` of layout `id`; `None` for a bad id or past the end.
pub fn layout_word(id: i32, index: i32) -> Option<i32> {
    let index = usize::try_from(index).ok()?;
    table()?.layout(id as u32)?.word(index).map(|w| w as i32)
}

/// Word `index` of style `id`, or the stream's length for `index` -1;
/// `None` for a bad id or past the end.
pub fn style_word(id: i32, index: i32) -> Option<i32> {
    let words = table()?.style(id as u32)?;
    if index == -1 {
        return i32::try_from(words.len()).ok();
    }
    words.word(usize::try_from(index).ok()?).map(|w| w as i32)
}

/// The ASSETS entry name behind drawable resource `id`.
pub fn drawable_name(id: i32) -> Option<&'static str> {
    core::str::from_utf8(table()?.drawable_name(id as u32)?).ok()
}
