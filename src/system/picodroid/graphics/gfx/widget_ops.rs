//! Per-widget sub-trait stubs.
//!
//! Each widget gets its own `*Ops` sub-trait with only the operations that
//! widget alone uses. Cross-widget ops (set_pos, set_size, …) live on
//! [`super::Gfx`].
//!
//! Sub-traits are intentionally empty here — methods are filled in as each
//! widget is migrated (step 7 of the plan). An empty `&mut dyn LabelOps`
//! compiles fine and acts as a structural marker until then.

use super::handle::Handle;

#[allow(dead_code)]
pub trait LabelOps {
    fn set_text(&mut self, _h: Handle, _text: &str) {}
}

#[allow(dead_code)]
pub trait TextViewOps {}

#[allow(dead_code)]
pub trait ButtonOps {}

#[allow(dead_code)]
pub trait ProgressBarOps {}

#[allow(dead_code)]
pub trait CheckBoxOps {}

#[allow(dead_code)]
pub trait SwitchOps {}

#[allow(dead_code)]
pub trait ToggleButtonOps {}

#[allow(dead_code)]
pub trait SeekBarOps {}

#[allow(dead_code)]
pub trait SpinnerOps {}

#[allow(dead_code)]
pub trait ImageViewOps {}

#[allow(dead_code)]
pub trait LinearLayoutOps {}

#[allow(dead_code)]
pub trait FrameLayoutOps {}

#[allow(dead_code)]
pub trait ScrollViewOps {}

#[allow(dead_code)]
pub trait ListViewOps {}

#[allow(dead_code)]
pub trait EditTextOps {}
