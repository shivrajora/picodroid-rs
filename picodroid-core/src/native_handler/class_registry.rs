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
pub const PICODROID_NATIVE_CLASSES: &[&str] = &[
    "picodroid/pio/Adc",
    "picodroid/pio/Gpio",
    "picodroid/pio/I2cDevice",
    "picodroid/pio/PeripheralManager",
    "picodroid/pio/Pwm",
    "picodroid/pio/SpiDevice",
    "picodroid/pio/UartDevice",
    "picodroid/os/SystemClock",
    "picodroid/os/Runtime",
    "picodroid/debug/DisplayDebug",
    "picodroid/util/Log",
    "picodroid/concurrent/Thread",
    "picodroid/concurrent/Executor",
    "picodroid/concurrent/Executors",
    "picodroid/concurrent/MainExecutor",
    "picodroid/concurrent/BackgroundExecutor",
    "picodroid/app/Application",
    "picodroid/app/Activity",
    "picodroid/app/Service",
    "picodroid/os/IBinder",
    "picodroid/app/Notification",
    "picodroid/app/NotificationManager",
    "picodroid/content/Context",
    "picodroid/content/Intent",
    "picodroid/content/ServiceConnection",
    "picodroid/content/pm/PackageManager",
    "picodroid/view/View",
    "picodroid/view/ViewGroup",
    "picodroid/view/MotionEvent",
    "picodroid/view/KeyEvent",
    "picodroid/view/OnKeyListener",
    "picodroid/view/OnSwipeListener",
    "picodroid/view/OnTouchListener",
    "picodroid/view/GestureDetector",
    "picodroid/view/GestureDetector$OnGestureListener",
    "picodroid/view/ViewPropertyAnimator",
    "picodroid/graphics/Theme",
    "picodroid/graphics/drawable/Drawable",
    "picodroid/graphics/drawable/GradientDrawable",
    "picodroid/graphics/drawable/GradientDrawable$Orientation",
    "picodroid/graphics/Display",
    "picodroid/widget/TextView",
    "picodroid/widget/Button",
    "picodroid/widget/LinearLayout",
    "picodroid/widget/ProgressBar",
    "picodroid/widget/Switch",
    "picodroid/widget/ListView",
    "picodroid/widget/NumberPicker",
    "picodroid/widget/ImageView",
    "picodroid/widget/ToggleButton",
    "picodroid/widget/CompoundButton",
    "picodroid/widget/SeekBar",
    "picodroid/widget/CheckBox",
    "picodroid/widget/RadioButton",
    "picodroid/widget/ScrollView",
    "picodroid/widget/FrameLayout",
    "picodroid/widget/Spinner",
    "picodroid/widget/DatePicker",
    "picodroid/widget/TimePicker",
    "picodroid/widget/EditText",
    "picodroid/widget/Toast",
    "picodroid/widget/Snackbar",
    "picodroid/widget/SwipeRefreshLayout",
    "picodroid/app/AlertDialog",
    "picodroid/app/AlertDialog$Builder",
    "picodroid/widget/Keyboard",
    "picodroid/net/Socket",
    "picodroid/net/ServerSocket",
    "picodroid/net/DatagramSocket",
    "picodroid/net/DatagramPacket",
    "picodroid/net/InetAddress",
    "picodroid/net/NetworkInfo",
    "picodroid/net/URL",
    "picodroid/net/HttpURLConnection",
    "picodroid/net/HttpInputStream",
    "picodroid/net/HttpOutputStream",
    "picodroid/io/File",
    "picodroid/io/FileInputStream",
    "picodroid/io/FileOutputStream",
    "picodroid/hardware/Sensor",
    "picodroid/hardware/SensorEvent",
    "picodroid/hardware/SensorEventListener",
    "picodroid/hardware/SensorManager",
];

/// (class, method) → one-line hint pointing at the picodroid equivalent for an
/// Android idiom that picodroid deliberately omits. Consulted on a native-miss
/// (the dispatch fall-through in [`super`]'s `dispatch`) so the NoSuchMethod a
/// ported app would otherwise see comes with an actionable alternative instead
/// of a bare class/method name. Keep it terse — it lives in flash. Class names
/// are the un-shrunk `picodroid/*` form (the miss site un-shrinks first).
pub const API_HINTS: &[(&str, &str, &str)] = &[
    (
        "picodroid/app/Activity",
        "runOnUiThread",
        "use Executors.mainExecutor().execute(Runnable)",
    ),
    (
        "picodroid/app/Activity",
        "findViewById",
        "no resource IDs — keep your View references, or use View.setTag/getTag",
    ),
    (
        "picodroid/view/View",
        "findViewById",
        "no resource IDs — keep your View references, or use setTag/getTag",
    ),
    (
        "picodroid/view/View",
        "post",
        "use Executors.mainExecutor().execute(Runnable)",
    ),
    (
        "picodroid/view/View",
        "postDelayed",
        "no Handler — use ViewPropertyAnimator timers or Executors.mainExecutor()",
    ),
    (
        "picodroid/app/Activity",
        "getLayoutInflater",
        "no XML layouts — build Views programmatically",
    ),
    (
        "picodroid/app/Activity",
        "getResources",
        "no Resources — bundle files under assets/ and use the generated AssetConstants",
    ),
    (
        "picodroid/content/Context",
        "getResources",
        "no Resources — bundle files under assets/ and use the generated AssetConstants",
    ),
    (
        "picodroid/content/Context",
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
    use pico_jvm::class_file::ClassFile;
    use pico_jvm::native::BUILTIN_CLASS_NAMES;

    #[test]
    fn api_hint_lookup() {
        assert_eq!(
            api_hint("picodroid/app/Activity", "runOnUiThread"),
            Some("use Executors.mainExecutor().execute(Runnable)")
        );
        // Unknown (class, method) → no hint.
        assert_eq!(api_hint("picodroid/app/Activity", "onCreate"), None);
        // getSystemService is implemented now, so it must not carry a hint
        // telling callers to avoid it.
        assert_eq!(
            api_hint("picodroid/content/Context", "getSystemService"),
            None
        );
        assert_eq!(api_hint("picodroid/widget/TextView", "setText"), None);
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
            let original = crate::shrink_names::unshrink_class(loaded);
            if !(PICODROID_NATIVE_CLASSES.contains(&original)
                || BUILTIN_CLASS_NAMES.contains(&original)
                || ALLOWED_UNREGISTERED.contains(&original))
            {
                missing.push(original);
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
