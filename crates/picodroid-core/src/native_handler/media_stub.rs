// SPDX-License-Identifier: GPL-3.0-only
//! `picodroid.media` on a board with no sound output.
//!
//! The SDK classes ship on every board (see `board_cfg::framework_class_excludes`
//! for why they are not board-excluded the way `picodroid.json` is), so their
//! natives must resolve everywhere or an app naming `ToneGenerator` on a silent
//! board dies with `NoSuchMethod` instead of simply staying quiet. This is the
//! shape `net_stub.rs` gives a board with no radio.
//!
//! Nothing here throws. `startTone` reporting `false` is already Android's
//! answer for a tone the platform will not play, so an app that checks the
//! return value needs no picodroid-specific code to run on a board with no
//! buzzer, and one that ignores it is merely silent.

use crate::shrink_names::c;
use crate::shrink_names::m;
use pico_jvm::{
    types::{JvmError, Value},
    NativeContext,
};

pub fn dispatch(
    class_name: &str,
    method_name: &str,
    _ctx: &mut NativeContext<'_>,
) -> Option<Result<Option<Value>, JvmError>> {
    if class_name != c::picodroid_media_ToneGenerator {
        return None;
    }
    let r = match method_name {
        // There is no output to configure, and no tone that can play.
        m::nativeInit | m::stopTone | m::release => Ok(None),
        m::startTone | m::startToneSequence => Ok(Some(Value::Int(0))),
        _ => return None,
    };
    Some(r)
}
