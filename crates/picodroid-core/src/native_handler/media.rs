// SPDX-License-Identifier: GPL-3.0-only
//! `picodroid.media` native dispatch.
//!
//! Present only on a board with an `[audio]` section (`cfg(has_audio)`). A
//! board without one also drops the SDK classes from its embedded framework,
//! so there is no stub counterpart to this module the way `net_stub.rs`
//! answers for a board with no radio: nothing can reach these methods, because
//! the classes that declare them are not there, and `is_excluded_on_this_board`
//! turns a stray reference into a message naming board.toml.

use crate::shrink_names::c;
use crate::shrink_names::m;
use pico_jvm::{
    types::{JvmError, Value},
    NativeContext,
};

pub fn dispatch(
    class_name: &str,
    method_name: &str,
    ctx: &mut NativeContext<'_>,
) -> Option<Result<Option<Value>, JvmError>> {
    if class_name != c::picodroid_media_ToneGenerator {
        return None;
    }
    let r = match method_name {
        m::nativeInit => crate::media::natives::native_init(ctx.args),
        m::startTone => crate::media::natives::start_tone(ctx.args),
        m::startToneSequence => crate::media::natives::start_tone_sequence(ctx.args, ctx.arrays),
        m::stopTone => crate::media::natives::stop_tone(ctx.args),
        m::release => crate::media::natives::release(ctx.args),
        _ => return None,
    };
    Some(r)
}
