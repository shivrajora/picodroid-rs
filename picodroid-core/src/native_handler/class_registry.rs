// SPDX-License-Identifier: GPL-3.0-only
//! Registry of picodroid framework classes with native methods.
//!
//! Hardware-free on purpose: `main.rs` re-includes this file via `#[path]`
//! under `cfg(test)` (the parent `native_handler` module is
//! `cfg(not(test))`-gated by its FFI/HAL imports) so the registry
//! cross-check below runs under `scripts/test.sh` in both shrink modes.

/// Picodroid framework class names the JVM must canonicalise to a stable
/// `&'static str` for pointer-identity caching. Returned from
/// `PicodroidNativeHandler::native_class_names` so the JVM never needs to
/// hardcode any `picodroid/*` names itself.
///
/// Add a class here whenever a new framework class becomes the receiver of a
/// virtual or static method call (i.e. anything dispatched through the
/// per-domain handlers in this module). Missing entries silently break
/// virtual dispatch; the `every_native_class_is_registered` test below fails
/// the build when an SDK class declares a `native` method without an entry
/// here (the bug class behind the `hardware/Sensor*` and `picodroid/pio/*`
/// registration misses, ca7e535 / 741f882).
use crate::shrink_names::c;
pub const PICODROID_NATIVE_CLASSES: &[&str] = &[
    c::picodroid_pio_Adc,
    c::picodroid_pio_Gpio,
    c::picodroid_pio_I2cDevice,
    c::picodroid_pio_PeripheralManager,
    c::picodroid_pio_Pwm,
    c::picodroid_pio_SpiDevice,
    c::picodroid_pio_UartDevice,
    c::picodroid_os_SystemClock,
    c::picodroid_os_Runtime,
    c::picodroid_debug_DisplayDebug,
    c::picodroid_util_Log,
    // JSONArray declares no natives of its own: it calls JSONObject's.
    c::picodroid_json_JSONObject,
    c::picodroid_concurrent_Thread,
    c::picodroid_concurrent_Executor,
    c::picodroid_concurrent_Executors,
    c::picodroid_concurrent_MainExecutor,
    c::picodroid_concurrent_BackgroundExecutor,
    c::picodroid_app_Application,
    c::picodroid_app_Activity,
    c::picodroid_app_Service,
    c::picodroid_os_IBinder,
    c::picodroid_app_Notification,
    c::picodroid_app_NotificationManager,
    c::picodroid_content_Context,
    c::picodroid_content_Intent,
    c::picodroid_content_ServiceConnection,
    c::picodroid_content_pm_PackageManager,
    c::picodroid_view_View,
    c::picodroid_view_ViewGroup,
    c::picodroid_view_MotionEvent,
    c::picodroid_view_KeyEvent,
    c::picodroid_view_OnKeyListener,
    c::picodroid_view_OnSwipeListener,
    c::picodroid_view_OnTouchListener,
    c::picodroid_view_GestureDetector,
    c::picodroid_view_GestureDetector_OnGestureListener,
    c::picodroid_view_ViewPropertyAnimator,
    c::picodroid_graphics_Theme,
    c::picodroid_graphics_drawable_Drawable,
    c::picodroid_graphics_drawable_GradientDrawable,
    c::picodroid_graphics_drawable_GradientDrawable_Orientation,
    c::picodroid_graphics_Display,
    c::picodroid_widget_TextView,
    c::picodroid_widget_Button,
    c::picodroid_widget_LinearLayout,
    c::picodroid_widget_ProgressBar,
    c::picodroid_widget_Switch,
    c::picodroid_widget_ListView,
    c::picodroid_widget_NumberPicker,
    c::picodroid_widget_ImageView,
    c::picodroid_widget_ToggleButton,
    c::picodroid_widget_CompoundButton,
    c::picodroid_widget_SeekBar,
    c::picodroid_widget_CheckBox,
    c::picodroid_widget_RadioButton,
    c::picodroid_widget_ScrollView,
    c::picodroid_widget_FrameLayout,
    c::picodroid_widget_Spinner,
    c::picodroid_widget_DatePicker,
    c::picodroid_widget_TimePicker,
    c::picodroid_widget_EditText,
    c::picodroid_widget_Toast,
    c::picodroid_widget_Snackbar,
    c::picodroid_widget_SwipeRefreshLayout,
    c::picodroid_app_AlertDialog,
    c::picodroid_app_AlertDialog_Builder,
    c::picodroid_widget_Keyboard,
    c::picodroid_net_Socket,
    c::picodroid_net_ServerSocket,
    c::picodroid_net_DatagramSocket,
    c::picodroid_net_DatagramPacket,
    c::picodroid_net_InetAddress,
    c::picodroid_net_NetworkInfo,
    c::picodroid_net_URL,
    c::picodroid_net_HttpURLConnection,
    c::picodroid_net_HttpInputStream,
    c::picodroid_net_HttpOutputStream,
    c::picodroid_io_File,
    c::picodroid_io_FileInputStream,
    c::picodroid_io_FileOutputStream,
    c::picodroid_hardware_Sensor,
    c::picodroid_hardware_SensorEvent,
    c::picodroid_hardware_SensorEventListener,
    c::picodroid_hardware_SensorManager,
];

/// (class, method) → one-line hint pointing at the picodroid equivalent for an
/// Android idiom that picodroid deliberately omits. Consulted on a native-miss
/// (the dispatch fall-through in [`super`]'s `dispatch`) so the NoSuchMethod a
/// ported app would otherwise see comes with an actionable alternative instead
/// of a bare class/method name. Keep it terse — it lives in flash. Class names
/// are the un-shrunk `picodroid/*` form (the miss site un-shrinks first).
pub const API_HINTS: &[(&str, &str, &str)] = &[
    (
        c::picodroid_app_Activity,
        "runOnUiThread",
        "use Executors.mainExecutor().execute(Runnable)",
    ),
    (
        c::picodroid_app_Activity,
        "findViewById",
        "no resource IDs — keep your View references, or use View.setTag/getTag",
    ),
    (
        c::picodroid_view_View,
        "findViewById",
        "no resource IDs — keep your View references, or use setTag/getTag",
    ),
    (
        c::picodroid_view_View,
        "post",
        "use Executors.mainExecutor().execute(Runnable)",
    ),
    (
        c::picodroid_view_View,
        "postDelayed",
        "no Handler — use ViewPropertyAnimator timers or Executors.mainExecutor()",
    ),
    (
        c::picodroid_app_Activity,
        "getLayoutInflater",
        "no XML layouts — build Views programmatically",
    ),
    (
        c::picodroid_app_Activity,
        "getResources",
        "no Resources — bundle files under assets/ and use the generated AssetConstants",
    ),
    (
        c::picodroid_content_Context,
        "getResources",
        "no Resources — bundle files under assets/ and use the generated AssetConstants",
    ),
    (
        c::picodroid_content_Context,
        "registerReceiver",
        "no BroadcastReceiver — use a bound Service or a direct callback",
    ),
];

/// Hint for a missing native `(class, method)`, or `None` if there is no
/// curated alternative. See [`API_HINTS`].
pub fn api_hint(class: &str, method: &str) -> Option<&'static str> {
    API_HINTS
        .iter()
        .find(|(c, m, _)| *c == class && *m == method)
        .map(|(_, _, hint)| *hint)
}

/// Whether `class` is an SDK class the active board left out of its embedded
/// framework via `framework_class_excludes` (board.toml). Distinguishes "this
/// board does not ship that class" from "picodroid has no such method" on a
/// native miss. Empty — and so always false — on boards that exclude nothing.
#[cfg(not(test))]
pub fn is_excluded_on_this_board(class: &str) -> bool {
    crate::framework_classes::FRAMEWORK_EXCLUDED_CLASSES
        .iter()
        .any(|c| *c == class || c.split('$').next() == Some(class))
}

#[cfg(test)]
mod tests {
    use super::{api_hint, API_HINTS, PICODROID_NATIVE_CLASSES};
    use crate::shrink_names::{c, m};
    use pico_jvm::class_file::ClassFile;
    use pico_jvm::native::BUILTIN_CLASS_NAMES;

    #[test]
    fn api_hint_lookup() {
        assert_eq!(
            api_hint(c::picodroid_app_Activity, "runOnUiThread"),
            Some("use Executors.mainExecutor().execute(Runnable)")
        );
        // Unknown (class, method) → no hint.
        assert_eq!(api_hint(c::picodroid_app_Activity, m::onCreate), None);
        // getSystemService is implemented now, so it must not carry a hint
        // telling callers to avoid it.
        assert_eq!(
            api_hint(c::picodroid_content_Context, m::getSystemService),
            None
        );
        assert_eq!(api_hint(c::picodroid_widget_TextView, m::setText), None);
    }

    /// A hint that names a non-existent picodroid class can never fire (the
    /// miss site un-shrinks to a real loaded name first), so it would be dead
    /// weight. Every hint's class must be a registered native class.
    #[test]
    fn api_hint_classes_are_registered() {
        for (class, method, _) in API_HINTS {
            assert!(
                PICODROID_NATIVE_CLASSES.contains(class),
                "API_HINTS references unregistered class {class:?} (method {method:?})"
            );
        }
    }

    /// JVMS §4.6 `ACC_NATIVE` method access flag.
    const ACC_NATIVE: u16 = 0x0100;

    /// Classes allowed to declare `native` methods without a registry entry.
    /// Must stay empty unless a method is *intentionally* unimplemented on
    /// this platform — every entry here is a runtime NoSuchMethod waiting to
    /// happen, so justify additions in a comment.
    const ALLOWED_UNREGISTERED: &[&str] = &[];

    /// Every SDK class that declares a `native` method must be registered in
    /// PICODROID_NATIVE_CLASSES (picodroid/*) or BUILTIN_CLASS_NAMES
    /// (java/*). An unregistered class compiles and boots fine but fails
    /// virtual dispatch at runtime with NoSuchMethod — historically only
    /// caught on device, in shrink mode, via the `native miss` defmt log.
    /// Runs under both shrink modes (scripts/test.sh): loaded names are
    /// un-shrunk before the registry lookup, exactly like the runtime path.
    #[test]
    fn every_native_class_is_registered() {
        let mut native_classes = 0;
        let mut missing: Vec<&str> = Vec::new();
        for bytes in crate::framework_classes::FRAMEWORK_CLASSES {
            let cf = ClassFile::parse(bytes).expect("parse framework class");
            let declares_native = cf
                .methods()
                .iter()
                .any(|m| m.access_flags & ACC_NATIVE != 0);
            if !declares_native {
                continue;
            }
            native_classes += 1;

            let loaded = core::str::from_utf8(cf.class_name().expect("class name"))
                .expect("class name is UTF-8");
            // The registries are spelled as loaded (`c::`), so the loaded
            // name compares directly — exactly what dispatch does.
            if !(PICODROID_NATIVE_CLASSES.contains(&loaded)
                || BUILTIN_CLASS_NAMES.contains(&loaded)
                || ALLOWED_UNREGISTERED.contains(&loaded))
            {
                missing.push(crate::shrink_names::unshrink_class(loaded));
            }
        }
        assert!(
            missing.is_empty(),
            "{} class(es) declare native methods but are missing from \
             PICODROID_NATIVE_CLASSES (platforms/rp/src/system/native_handler/\
             class_registry.rs) and BUILTIN_CLASS_NAMES (jvm/src/native/mod.rs) \
             — virtual dispatch on them will fail with NoSuchMethod at runtime: \
             {missing:?}",
            missing.len()
        );
        assert!(
            native_classes > 0,
            "no framework class declares native methods — FRAMEWORK_CLASSES \
             is empty or the parser lost method access flags; this test is \
             vacuous"
        );
    }

    /// No embedded `java/**` class may be pure-abstract — every one must
    /// declare at least one method this JVM could actually execute (a `Code`
    /// attribute) or dispatch natively (`ACC_NATIVE`).
    ///
    /// A body-less `java/**` file earns nothing and costs everywhere:
    ///
    /// - **Apps never see it.** Both the SDK and every app compile with
    ///   `javac --release 8` and no bootclasspath override (`build.gradle.kts`),
    ///   so `java.*` resolves from the JDK's `ct.sym`, which precedes the SDK
    ///   on the class path. A `sdk/java/java/util/Map.java` written "to
    ///   document the supported subset" documents nothing — javac silently
    ///   uses the JDK's `Map` instead.
    /// - **The runtime never reads it.** `invokevirtual`/`invokeinterface`
    ///   dispatch on the receiver's runtime class, not the constant pool's
    ///   declared owner (`jvm/src/interpreter/ops_invoke.rs`), and
    ///   `instanceof`/`checkcast` walk `BUILTIN_INTERFACES`
    ///   (`jvm/src/interpreter/helpers.rs`), tolerating interfaces that have
    ///   no class file at all.
    /// - **Every board pays.** The SDK is embedded whole and loaded at boot
    ///   (`build_support/papk.rs`, `boot.rs`), so it is flash on the RP2040
    ///   too, whose program region has ~20 KB free against a 0 %-growth
    ///   ratchet.
    ///
    /// `javax/**` is exempt: it is not in `ct.sym`, so `javax/inject/Provider`
    /// genuinely must ship a class file for apps to compile against.
    #[test]
    fn no_bodiless_java_framework_classes() {
        let mut java_classes = 0;
        let mut bodiless: Vec<(&str, usize)> = Vec::new();
        for bytes in crate::framework_classes::FRAMEWORK_CLASSES {
            let cf = ClassFile::parse(bytes).expect("parse framework class");
            let loaded = core::str::from_utf8(cf.class_name().expect("class name"))
                .expect("class name is UTF-8");
            // Un-shrink first: `java/**` is kept verbatim by sdk/keep.toml
            // today, but if that ever changed a shrunk name would slip past
            // the prefix filter and quietly make this test vacuous.
            let original = crate::shrink_names::unshrink_class(loaded);
            if !original.starts_with("java/") {
                continue;
            }
            java_classes += 1;
            // A method has a Code attribute iff it is neither abstract nor
            // native (JVMS §4.7.3); `code_offset` is 0 when the parser found
            // none — the same "has a body" signal `find_default_method` uses.
            let has_body = cf
                .methods()
                .iter()
                .any(|m| m.code_offset != 0 || m.access_flags & ACC_NATIVE != 0);
            if !has_body {
                bodiless.push((original, bytes.len()));
            }
        }
        assert!(
            bodiless.is_empty(),
            "{} embedded java/** class(es) declare no method with a Code \
             attribute or ACC_NATIVE: {:?} (name, .class bytes). javac \
             --release 8 resolves java.* from the JDK's ct.sym, so no app \
             compiles against these files, and dispatch goes by the receiver's \
             runtime class, so the JVM never reads them — they are pure flash \
             on every board. Delete the .java file; if user code needs the \
             name as a lambda SAM, an instanceof/checkcast target, or a \
             superinterface edge, add a BUILTIN_CLASS_NAMES row \
             (jvm/src/native/mod.rs) or a BUILTIN_INTERFACES row \
             (jvm/src/interpreter/helpers.rs) instead.",
            bodiless.len(),
            bodiless
        );
        assert!(
            java_classes > 0,
            "no java/** framework class seen — FRAMEWORK_CLASSES is empty \
             (PICODROID_APK_PATH unset? use scripts/test.sh) or the prefix \
             filter broke; this test is vacuous"
        );
    }
}
