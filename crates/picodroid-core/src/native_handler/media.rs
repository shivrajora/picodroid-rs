// SPDX-License-Identifier: GPL-3.0-only
//! `picodroid.media` native dispatch.
//!
//! Present only on a board with an `[audio]` section (`cfg(has_audio)`). A
//! board without one also drops the SDK classes from its embedded framework,
//! so there is no stub counterpart to this module the way `net_stub.rs`
//! answers for a board with no radio: nothing can reach these methods, because
//! the classes that declare them are not there, and `is_excluded_on_this_board`
//! turns a stray reference into a message naming board.toml.
use crate::shrink_names::{ch, mh};
use pico_jvm::NativeKey;

use pico_jvm::{
    types::{JvmError, Value},
    NativeContext,
};

pub fn dispatch(
    key: NativeKey,
    ctx: &mut NativeContext<'_>,
) -> Option<Result<Option<Value>, JvmError>> {
    if key.class != ch::picodroid_media_ToneGenerator {
        return None;
    }
    let r = match key.method {
        mh::nativeInit => crate::media::natives::native_init(ctx.args),
        mh::startTone => crate::media::natives::start_tone(ctx.args),
        mh::startToneSequence => crate::media::natives::start_tone_sequence(ctx.args, ctx.arrays),
        mh::stopTone => crate::media::natives::stop_tone(ctx.args),
        mh::release => crate::media::natives::release(ctx.args),
        _ => return None,
    };
    Some(r)
}
