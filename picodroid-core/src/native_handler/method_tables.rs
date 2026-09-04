// SPDX-License-Identifier: GPL-3.0-only
//! Method-level native-dispatch cross-check (audit P1-6, quality-roadmap
//! stage 2 — the method-level extension of `every_native_class_is_registered`
//! in `class_registry.rs`).
//!
//! Every `(declaring class, method, descriptor)` triple the native dispatch
//! surface handles is declared ONCE here, per handler module, and one
//! bidirectional test diffs the union against the SDK's `ACC_NATIVE`
//! methods:
//!
//! * **Direction A** — every SDK `native` method is handled (or explicitly
//!   allowlisted): a missing arm is a silent runtime `NoSuchMethod`, the
//!   most frequent serious bug class in this repo's history. The failure
//!   message prints ready-to-paste rows — that IS the migration workflow.
//! * **Direction B** — every table row matches a real SDK declaration:
//!   catches descriptor typos, overload drift, and stale rows after an SDK
//!   rename.
//!
//! Rows are keyed by **declaring class** (what the class files enumerate).
//! Wildcard-receiver arms (`(_, "startActivity")`) list one row per
//! declaring class; inherited-View routing needs no inheritance modeling.
//! Note for hand-editing: `AlertDialog`'s natives split across
//! `picodroid/app/AlertDialog` and `picodroid/app/AlertDialog$Builder` —
//! paste from the test failure output rather than transcribing .java files.
//!
//! This file is wired ONLY via the `#[cfg(test)] #[path]` include in
//! `main.rs` — a `not(test)` `mod` declaration would fail the `-D warnings`
//! clippy lanes (dead pub consts under the private `system::` tree), and
//! nothing at runtime may reference these tables anyway: the RP2040 sits at
//! its 896K flash ceiling and ~310 string triples must never reach
//! `.rodata`.

/// (declaring class, method name, descriptor) — original (un-shrunk) names.
pub type Row = (&'static str, &'static str, &'static str);

// ── Per-handler tables ──────────────────────────────────────────────────────
// One table per dispatch module; a row lives in the table of the module
// whose `match` owns the arm. `ALL_HANDLED` unions them; the test rejects
// duplicate rows across tables.

/// `native_handler/pio.rs`
pub const PIO_HANDLED: &[Row] = &[
    // picodroid/pio/Adc
    ("picodroid/pio/Adc", "close", "()V"),
    ("picodroid/pio/Adc", "readValue", "()D"),
    // picodroid/pio/Gpio
    ("picodroid/pio/Gpio", "close", "()V"),
    ("picodroid/pio/Gpio", "getValue", "()Z"),
    ("picodroid/pio/Gpio", "setDirection", "(I)V"),
    ("picodroid/pio/Gpio", "setValue", "(Z)V"),
    // picodroid/pio/I2cDevice
    ("picodroid/pio/I2cDevice", "close", "()V"),
    ("picodroid/pio/I2cDevice", "read", "(I[BI)I"),
    ("picodroid/pio/I2cDevice", "setSpeed", "(I)V"),
    ("picodroid/pio/I2cDevice", "write", "(I[BI)I"),
    // picodroid/pio/PeripheralManager
    (
        "picodroid/pio/PeripheralManager",
        "getInstance",
        "()Lpicodroid/pio/PeripheralManager;",
    ),
    (
        "picodroid/pio/PeripheralManager",
        "openAdcPin",
        "(Ljava/lang/String;)Lpicodroid/pio/Adc;",
    ),
    (
        "picodroid/pio/PeripheralManager",
        "openGpio",
        "(Ljava/lang/String;)Lpicodroid/pio/Gpio;",
    ),
    (
        "picodroid/pio/PeripheralManager",
        "openI2cDevice",
        "(Ljava/lang/String;)Lpicodroid/pio/I2cDevice;",
    ),
    (
        "picodroid/pio/PeripheralManager",
        "openPwm",
        "(Ljava/lang/String;)Lpicodroid/pio/Pwm;",
    ),
    (
        "picodroid/pio/PeripheralManager",
        "openSpiDevice",
        "(Ljava/lang/String;)Lpicodroid/pio/SpiDevice;",
    ),
    (
        "picodroid/pio/PeripheralManager",
        "openUartDevice",
        "(Ljava/lang/String;)Lpicodroid/pio/UartDevice;",
    ),
    // picodroid/pio/Pwm
    ("picodroid/pio/Pwm", "close", "()V"),
    ("picodroid/pio/Pwm", "setEnabled", "(Z)V"),
    ("picodroid/pio/Pwm", "setPwmDutyCycle", "(D)V"),
    ("picodroid/pio/Pwm", "setPwmFrequencyHz", "(D)V"),
    // picodroid/pio/SpiDevice
    ("picodroid/pio/SpiDevice", "close", "()V"),
    ("picodroid/pio/SpiDevice", "setFrequency", "(I)V"),
    ("picodroid/pio/SpiDevice", "setMode", "(I)V"),
    ("picodroid/pio/SpiDevice", "transfer", "([B[BI)I"),
    ("picodroid/pio/SpiDevice", "write", "([BI)I"),
    // picodroid/pio/UartDevice
    ("picodroid/pio/UartDevice", "close", "()V"),
    ("picodroid/pio/UartDevice", "readByte", "()I"),
    ("picodroid/pio/UartDevice", "setBaudrate", "(I)V"),
    ("picodroid/pio/UartDevice", "setDataSize", "(I)V"),
    ("picodroid/pio/UartDevice", "setHardwareFlowControl", "(I)V"),
    ("picodroid/pio/UartDevice", "setParity", "(I)V"),
    ("picodroid/pio/UartDevice", "setStopBits", "(I)V"),
    ("picodroid/pio/UartDevice", "writeByte", "(I)I"),
];

/// `native_handler/io.rs`
pub const IO_HANDLED: &[Row] = &[
    // picodroid/io/File
    ("picodroid/io/File", "createNewFile", "()Z"),
    ("picodroid/io/File", "delete", "()Z"),
    ("picodroid/io/File", "exists", "()Z"),
    ("picodroid/io/File", "isDirectory", "()Z"),
    ("picodroid/io/File", "isFile", "()Z"),
    ("picodroid/io/File", "length", "()J"),
    ("picodroid/io/File", "mkdir", "()Z"),
    ("picodroid/io/File", "renameTo", "(Lpicodroid/io/File;)Z"),
    // picodroid/io/FileInputStream
    ("picodroid/io/FileInputStream", "available", "()I"),
    ("picodroid/io/FileInputStream", "read", "([BII)I"),
    // picodroid/io/FileOutputStream
    ("picodroid/io/FileOutputStream", "flush", "()V"),
    (
        "picodroid/io/FileOutputStream",
        "initStream",
        "(Ljava/lang/String;Z)J",
    ),
    ("picodroid/io/FileOutputStream", "write", "([BII)V"),
];

/// `native_handler/net.rs` (+ `net_stub.rs`, whose catch-all serves the
/// same surface on `not(has_network)` boards — this table is cfg-free and
/// asserts the union surface).
pub const NET_HANDLED: &[Row] = &[
    // picodroid/net/DatagramSocket
    ("picodroid/net/DatagramSocket", "close", "()V"),
    ("picodroid/net/DatagramSocket", "nativeCreate", "(I)I"),
    (
        "picodroid/net/DatagramSocket",
        "receive",
        "(Lpicodroid/net/DatagramPacket;)V",
    ),
    (
        "picodroid/net/DatagramSocket",
        "send",
        "(Lpicodroid/net/DatagramPacket;)V",
    ),
    ("picodroid/net/DatagramSocket", "setTimeout", "(I)V"),
    // picodroid/net/HttpInputStream
    ("picodroid/net/HttpInputStream", "read", "([BII)I"),
    // picodroid/net/HttpOutputStream
    ("picodroid/net/HttpOutputStream", "write", "([BII)V"),
    // picodroid/net/HttpURLConnection
    (
        "picodroid/net/HttpURLConnection",
        "nativeConnect",
        "(Ljava/lang/String;ILjava/lang/String;Ljava/lang/String;IIILjava/lang/String;)I",
    ),
    (
        "picodroid/net/HttpURLConnection",
        "nativeContentLength",
        "(I)I",
    ),
    (
        "picodroid/net/HttpURLConnection",
        "nativeDisconnect",
        "(I)V",
    ),
    (
        "picodroid/net/HttpURLConnection",
        "nativeHeaderField",
        "(ILjava/lang/String;)Ljava/lang/String;",
    ),
    (
        "picodroid/net/HttpURLConnection",
        "nativeHeaderFieldAt",
        "(IIZ)Ljava/lang/String;",
    ),
    (
        "picodroid/net/HttpURLConnection",
        "nativeReadResponseCode",
        "(I)I",
    ),
    (
        "picodroid/net/HttpURLConnection",
        "nativeResponseMessage",
        "(I)Ljava/lang/String;",
    ),
    // picodroid/net/InetAddress
    (
        "picodroid/net/InetAddress",
        "getHostAddress",
        "()Ljava/lang/String;",
    ),
    (
        "picodroid/net/InetAddress",
        "nativeResolve",
        "(Ljava/lang/String;)I",
    ),
    // picodroid/net/NetworkInfo
    ("picodroid/net/NetworkInfo", "getIpAddress", "()I"),
    ("picodroid/net/NetworkInfo", "getType", "()I"),
    ("picodroid/net/NetworkInfo", "isConnected", "()Z"),
    // picodroid/net/ServerSocket
    (
        "picodroid/net/ServerSocket",
        "accept",
        "()Lpicodroid/net/Socket;",
    ),
    ("picodroid/net/ServerSocket", "close", "()V"),
    ("picodroid/net/ServerSocket", "nativeListen", "(I)I"),
    ("picodroid/net/ServerSocket", "setSoTimeout", "(I)V"),
    // picodroid/net/Socket
    ("picodroid/net/Socket", "close", "()V"),
    ("picodroid/net/Socket", "connect", "(II)V"),
    ("picodroid/net/Socket", "nativeCreate", "()I"),
    ("picodroid/net/Socket", "recv", "([BII)I"),
    ("picodroid/net/Socket", "send", "([BII)I"),
    ("picodroid/net/Socket", "setTimeout", "(I)V"),
];

/// `native_handler/os.rs`
pub const OS_HANDLED: &[Row] = &[
    // java/lang/System
    ("java/lang/System", "currentTimeMillis", "()J"),
    // picodroid/content/pm/PackageManager
    (
        "picodroid/content/pm/PackageManager",
        "hasSystemFeature",
        "(Ljava/lang/String;)Z",
    ),
    // picodroid/os/SystemClock
    ("picodroid/os/SystemClock", "elapsedRealtimeNanos", "()J"),
    ("picodroid/os/SystemClock", "setCurrentTimeMillis", "(J)Z"),
    ("picodroid/os/SystemClock", "sleep", "(I)V"),
];

/// `native_handler/concurrent.rs`
pub const CONCURRENT_HANDLED: &[Row] = &[
    // picodroid/concurrent/BackgroundExecutor
    (
        "picodroid/concurrent/BackgroundExecutor",
        "execute",
        "(Ljava/lang/Runnable;)V",
    ),
    // picodroid/concurrent/Executors
    (
        "picodroid/concurrent/Executors",
        "backgroundExecutor",
        "()Lpicodroid/concurrent/Executor;",
    ),
    (
        "picodroid/concurrent/Executors",
        "mainExecutor",
        "()Lpicodroid/concurrent/Executor;",
    ),
    // picodroid/concurrent/MainExecutor
    (
        "picodroid/concurrent/MainExecutor",
        "execute",
        "(Ljava/lang/Runnable;)V",
    ),
];

/// `native_handler/sensors.rs`
/// `native_handler/threads.rs`. `Object.wait`/`notify`/`notifyAll` are
/// served there too but have no row here: `java/lang/Object` is a builtin
/// with no SDK class file to cross-check against. They are recorded in
/// `PLATFORM_BUILTIN_METHODS` below instead, which feeds the generated
/// compile-time contract.
pub const THREADS_HANDLED: &[Row] = &[
    // picodroid/concurrent/Thread
    ("picodroid/concurrent/Thread", "adopt0", "()V"),
    (
        "picodroid/concurrent/Thread",
        "current0",
        "()Lpicodroid/concurrent/Thread;",
    ),
    ("picodroid/concurrent/Thread", "currentKind0", "()I"),
    ("picodroid/concurrent/Thread", "exit0", "()V"),
    ("picodroid/concurrent/Thread", "interrupt", "()V"),
    ("picodroid/concurrent/Thread", "interrupted", "()Z"),
    ("picodroid/concurrent/Thread", "isAlive", "()Z"),
    ("picodroid/concurrent/Thread", "isInterrupted", "()Z"),
    ("picodroid/concurrent/Thread", "join0", "(J)V"),
    ("picodroid/concurrent/Thread", "sleep0", "(J)V"),
    ("picodroid/concurrent/Thread", "start0", "()Z"),
    ("picodroid/concurrent/Thread", "yield0", "()V"),
];

/// Methods of classfile-less `java/**` builtins that the *platform* handler
/// serves rather than pico-jvm's `BuiltinHandler` — the embedder's half of
/// `pico_jvm::native::BUILTIN_METHODS`, same row shape. Today that is only
/// `Object.wait`/`notify`/`notifyAll` (`native_handler/threads.rs`, matched
/// on name + descriptor for any receiver; `wait(long, int)` is not served).
/// Outside the `ACC_NATIVE` diff above by construction; consumed by the API
/// contract generator (`api_contract.rs`).
pub const PLATFORM_BUILTIN_METHODS: &[(&str, &[pico_jvm::native::BuiltinMethodRow])] = &[(
    "java/lang/Object",
    &[
        ("wait", &["()V", "(J)V"]),
        ("notify", &["()V"]),
        ("notifyAll", &["()V"]),
    ],
)];

pub const SENSORS_HANDLED: &[Row] = &[
    // picodroid/hardware/SensorManager
    (
        "picodroid/hardware/SensorManager",
        "getDefaultSensor",
        "(I)Lpicodroid/hardware/Sensor;",
    ),
    (
        "picodroid/hardware/SensorManager",
        "registerListener",
        "(Lpicodroid/hardware/SensorEventListener;Lpicodroid/hardware/Sensor;I)Z",
    ),
    (
        "picodroid/hardware/SensorManager",
        "unregisterListener",
        "(Lpicodroid/hardware/SensorEventListener;)V",
    ),
];

/// `native_handler/app_services.rs`
pub const APP_SERVICES_HANDLED: &[Row] = &[
    // picodroid/app/NotificationManager
    ("picodroid/app/NotificationManager", "cancel", "(I)V"),
    (
        "picodroid/app/NotificationManager",
        "notify",
        "(ILpicodroid/app/Notification;)V",
    ),
    // picodroid/app/Service
    (
        "picodroid/app/Service",
        "startForeground",
        "(ILpicodroid/app/Notification;)V",
    ),
    ("picodroid/app/Service", "stopForeground", "(Z)V"),
    ("picodroid/app/Service", "stopSelf", "()V"),
    ("picodroid/app/Service", "stopSelfResult", "(I)Z"),
    // picodroid/content/Context
    (
        "picodroid/content/Context",
        "bindService",
        "(Lpicodroid/content/Intent;Lpicodroid/content/ServiceConnection;)V",
    ),
    (
        "picodroid/content/Context",
        "startService",
        "(Lpicodroid/content/Intent;)V",
    ),
    (
        "picodroid/content/Context",
        "stopService",
        "(Lpicodroid/content/Intent;)V",
    ),
    (
        "picodroid/content/Context",
        "unbindService",
        "(Lpicodroid/content/ServiceConnection;)V",
    ),
];

/// `native_handler/mod.rs` self-arms (Log, Runtime, Activity wildcard ops…)
pub const CORE_HANDLED: &[Row] = &[
    // picodroid/app/Activity
    ("picodroid/app/Activity", "finish", "()V"),
    (
        "picodroid/app/Activity",
        "getIntent",
        "()Lpicodroid/content/Intent;",
    ),
    ("picodroid/app/Activity", "setResult", "(I)V"),
    (
        "picodroid/app/Activity",
        "setResult",
        "(ILpicodroid/content/Intent;)V",
    ),
    (
        "picodroid/app/Activity",
        "startActivity",
        "(Lpicodroid/content/Intent;)V",
    ),
    (
        "picodroid/app/Activity",
        "startActivityForResult",
        "(Lpicodroid/content/Intent;I)V",
    ),
    // picodroid/app/Application
    (
        "picodroid/app/Application",
        "startActivity",
        "(Lpicodroid/content/Intent;)V",
    ),
    // picodroid/os/Runtime
    ("picodroid/os/Runtime", "gcCount", "()I"),
    ("picodroid/os/Runtime", "gcFreed", "()I"),
    ("picodroid/os/Runtime", "gcTimeNanos", "()J"),
    ("picodroid/os/Runtime", "peakMemory", "()J"),
    ("picodroid/os/Runtime", "resetGcStats", "()V"),
    ("picodroid/os/Runtime", "resetPeakMemory", "()V"),
    ("picodroid/os/Runtime", "usedMemory", "()J"),
    // picodroid/util/Log
    (
        "picodroid/util/Log",
        "d",
        "(Ljava/lang/String;Ljava/lang/String;)V",
    ),
    (
        "picodroid/util/Log",
        "e",
        "(Ljava/lang/String;Ljava/lang/String;)V",
    ),
    (
        "picodroid/util/Log",
        "i",
        "(Ljava/lang/String;Ljava/lang/String;)V",
    ),
    (
        "picodroid/util/Log",
        "v",
        "(Ljava/lang/String;Ljava/lang/String;)V",
    ),
    (
        "picodroid/util/Log",
        "w",
        "(Ljava/lang/String;Ljava/lang/String;)V",
    ),
];

/// `graphics/mod.rs` routing + `graphics/lvgl_backend.rs` dispatch_* arms.
pub const GRAPHICS_HANDLED: &[Row] = &[
    // picodroid/app/AlertDialog
    ("picodroid/app/AlertDialog", "nativeCreate", "(Ljava/lang/String;Ljava/lang/String;Ljava/lang/String;Ljava/lang/String;Ljava/lang/String;)I"),
    ("picodroid/app/AlertDialog", "nativeCreateWithList", "(Ljava/lang/String;Ljava/lang/String;Ljava/lang/String;Ljava/lang/String;Ljava/lang/String;Ljava/lang/String;II)I"),
    ("picodroid/app/AlertDialog", "nativeDismiss", "(I)V"),
    ("picodroid/app/AlertDialog", "nativePerformItemClick", "(II)V"),
    ("picodroid/app/AlertDialog", "nativeRegisterButtonClickListener", "()V"),
    ("picodroid/app/AlertDialog", "nativeShow", "(I)V"),
    // picodroid/debug/DisplayDebug
    ("picodroid/debug/DisplayDebug", "calibrate", "()V"),
    ("picodroid/debug/DisplayDebug", "pollTouch", "()Lpicodroid/view/MotionEvent;"),
    ("picodroid/debug/DisplayDebug", "showFps", "()V"),
    // picodroid/graphics/Display
    ("picodroid/graphics/Display", "getInstance", "()Lpicodroid/graphics/Display;"),
    ("picodroid/graphics/Display", "setContentView", "(Lpicodroid/view/View;)V"),
    ("picodroid/graphics/Display", "update", "()V"),
    // picodroid/graphics/drawable/GradientDrawable
    ("picodroid/graphics/drawable/GradientDrawable", "nativeApply", "(Lpicodroid/view/View;IIIIIIII)V"),
    // picodroid/view/View
    ("picodroid/view/View", "close", "()V"),
    ("picodroid/view/View", "getHeight", "()I"),
    ("picodroid/view/View", "getLeft", "()I"),
    ("picodroid/view/View", "getTop", "()I"),
    ("picodroid/view/View", "getWidth", "()I"),
    ("picodroid/view/View", "nativeGetProperty", "(I)F"),
    ("picodroid/view/View", "nativeIsFocused", "()Z"),
    ("picodroid/view/View", "nativeRegisterClickListener", "()V"),
    ("picodroid/view/View", "nativeRegisterFocusChangeListener", "()V"),
    ("picodroid/view/View", "nativeRegisterKeyListener", "()V"),
    ("picodroid/view/View", "nativeRegisterLongClickListener", "()V"),
    ("picodroid/view/View", "nativeRegisterSwipeListener", "()V"),
    ("picodroid/view/View", "nativeRegisterTouchListener", "()V"),
    ("picodroid/view/View", "nativeRequestFocus", "()Z"),
    ("picodroid/view/View", "nativeSetAlpha", "(F)V"),
    ("picodroid/view/View", "nativeSetEnabled", "(Z)V"),
    ("picodroid/view/View", "nativeSetFlexGrow", "(I)V"),
    ("picodroid/view/View", "nativeSetFocusable", "(Z)V"),
    ("picodroid/view/View", "nativeSetProperty", "(IF)V"),
    ("picodroid/view/View", "nativeSetVisibility", "(I)V"),
    ("picodroid/view/View", "performClick", "()V"),
    ("picodroid/view/View", "performLongClickNative", "()V"),
    ("picodroid/view/View", "setBackgroundColor", "(I)V"),
    ("picodroid/view/View", "setPadding", "(IIII)V"),
    ("picodroid/view/View", "setPosition", "(II)V"),
    ("picodroid/view/View", "setSize", "(II)V"),
    // picodroid/view/ViewGroup
    ("picodroid/view/ViewGroup", "addView", "(Lpicodroid/view/View;)V"),
    ("picodroid/view/ViewGroup", "getChildCount", "()I"),
    ("picodroid/view/ViewGroup", "removeAllViews", "()V"),
    ("picodroid/view/ViewGroup", "removeView", "(Lpicodroid/view/View;)V"),
    // picodroid/view/ViewPropertyAnimator
    ("picodroid/view/ViewPropertyAnimator", "nativeCancel", "(I)V"),
    ("picodroid/view/ViewPropertyAnimator", "nativeSetEndAction", "(ILjava/lang/Runnable;)V"),
    ("picodroid/view/ViewPropertyAnimator", "nativeStart", "(IIFIII)V"),
    // picodroid/widget/Button
    ("picodroid/widget/Button", "getText", "()Ljava/lang/CharSequence;"),
    ("picodroid/widget/Button", "nativeCreate", "(Ljava/lang/String;)I"),
    ("picodroid/widget/Button", "setText", "(Ljava/lang/String;)V"),
    // picodroid/widget/CheckBox
    ("picodroid/widget/CheckBox", "isChecked", "()Z"),
    ("picodroid/widget/CheckBox", "nativeCreate", "()I"),
    ("picodroid/widget/CheckBox", "setChecked", "(Z)V"),
    ("picodroid/widget/CheckBox", "setText", "(Ljava/lang/String;)V"),
    // picodroid/widget/CompoundButton
    ("picodroid/widget/CompoundButton", "nativeRegisterCheckedChangeListener", "()V"),
    ("picodroid/widget/CompoundButton", "performCheckedChange", "()V"),
    // picodroid/widget/DatePicker
    ("picodroid/widget/DatePicker", "getDay", "()I"),
    ("picodroid/widget/DatePicker", "getMonth", "()I"),
    ("picodroid/widget/DatePicker", "getYear", "()I"),
    ("picodroid/widget/DatePicker", "nativeCreate", "()I"),
    ("picodroid/widget/DatePicker", "nativeRegisterDateChangedListener", "()V"),
    ("picodroid/widget/DatePicker", "setDate", "(III)V"),
    // picodroid/widget/EditText
    ("picodroid/widget/EditText", "getText", "()Ljava/lang/String;"),
    ("picodroid/widget/EditText", "nativeCreate", "()I"),
    ("picodroid/widget/EditText", "nativeRegisterEditorActionListener", "()V"),
    ("picodroid/widget/EditText", "nativeRegisterTextChangedListener", "()V"),
    ("picodroid/widget/EditText", "setHint", "(Ljava/lang/String;)V"),
    ("picodroid/widget/EditText", "setInputType", "(I)V"),
    ("picodroid/widget/EditText", "setShowKeyboardOnTouch", "(Z)V"),
    ("picodroid/widget/EditText", "setText", "(Ljava/lang/String;)V"),
    // picodroid/widget/FrameLayout
    ("picodroid/widget/FrameLayout", "nativeCreate", "()I"),
    // picodroid/widget/ImageView
    ("picodroid/widget/ImageView", "nativeCreate", "()I"),
    ("picodroid/widget/ImageView", "setImageSource", "(Ljava/lang/String;)V"),
    ("picodroid/widget/ImageView", "setScale", "(I)V"),
    ("picodroid/widget/ImageView", "setScaleType", "(I)V"),
    ("picodroid/widget/ImageView", "setTint", "(I)V"),
    // picodroid/widget/Keyboard
    ("picodroid/widget/Keyboard", "nativeCreate", "()I"),
    ("picodroid/widget/Keyboard", "nativeRegisterReadyListener", "()V"),
    ("picodroid/widget/Keyboard", "nativeSetMode", "(I)V"),
    ("picodroid/widget/Keyboard", "nativeSetTextarea", "(Lpicodroid/widget/EditText;)V"),
    // picodroid/widget/LinearLayout
    ("picodroid/widget/LinearLayout", "nativeCreate", "()I"),
    ("picodroid/widget/LinearLayout", "setGravity", "(I)V"),
    ("picodroid/widget/LinearLayout", "setOrientation", "(I)V"),
    ("picodroid/widget/LinearLayout", "setSpacing", "(I)V"),
    // picodroid/widget/ListView
    ("picodroid/widget/ListView", "addItem", "(Ljava/lang/String;)V"),
    (
        "picodroid/widget/ListView",
        "nativeBindAdapter",
        "(Lpicodroid/widget/Adapter;)V",
    ),
    ("picodroid/widget/ListView", "nativeCreate", "()I"),
    ("picodroid/widget/ListView", "nativeRegisterItemClickListener", "()V"),
    // picodroid/widget/NumberPicker
    ("picodroid/widget/NumberPicker", "nativeCreate", "()I"),
    ("picodroid/widget/NumberPicker", "nativeRegisterPicker", "()V"),
    ("picodroid/widget/NumberPicker", "nativeSetText", "(Ljava/lang/String;)V"),
    // picodroid/widget/ProgressBar
    ("picodroid/widget/ProgressBar", "nativeCreate", "()I"),
    ("picodroid/widget/ProgressBar", "nativeCreateIndeterminate", "(I)I"),
    ("picodroid/widget/ProgressBar", "nativeSetProgress", "(I)V"),
    ("picodroid/widget/ProgressBar", "setTint", "(I)V"),
    // picodroid/widget/RadioButton
    ("picodroid/widget/RadioButton", "isChecked", "()Z"),
    ("picodroid/widget/RadioButton", "nativeCreate", "()I"),
    ("picodroid/widget/RadioButton", "setChecked", "(Z)V"),
    ("picodroid/widget/RadioButton", "setText", "(Ljava/lang/String;)V"),
    // picodroid/widget/ScrollView
    ("picodroid/widget/ScrollView", "nativeCreate", "()I"),
    // picodroid/widget/SeekBar
    ("picodroid/widget/SeekBar", "getProgress", "()I"),
    ("picodroid/widget/SeekBar", "nativeCreate", "()I"),
    ("picodroid/widget/SeekBar", "nativeCreateWithMax", "(I)I"),
    ("picodroid/widget/SeekBar", "nativeRegisterChangeListener", "()V"),
    ("picodroid/widget/SeekBar", "performProgressChange", "()V"),
    ("picodroid/widget/SeekBar", "performTrackingTouch", "()V"),
    ("picodroid/widget/SeekBar", "setMax", "(I)V"),
    ("picodroid/widget/SeekBar", "setProgress", "(I)V"),
    // picodroid/widget/Snackbar
    ("picodroid/widget/Snackbar", "nativeCreate", "(Ljava/lang/String;I)I"),
    ("picodroid/widget/Snackbar", "nativeDismiss", "(I)V"),
    ("picodroid/widget/Snackbar", "nativeRegisterActionClickListener", "()V"),
    ("picodroid/widget/Snackbar", "nativeSetAction", "(ILjava/lang/String;)V"),
    ("picodroid/widget/Snackbar", "nativeShow", "(I)V"),
    // picodroid/widget/Spinner
    ("picodroid/widget/Spinner", "getSelectedItemPosition", "()I"),
    ("picodroid/widget/Spinner", "nativeCreate", "()I"),
    ("picodroid/widget/Spinner", "nativeRegisterItemSelectedListener", "()V"),
    ("picodroid/widget/Spinner", "performItemSelected", "()V"),
    ("picodroid/widget/Spinner", "setItems", "(Ljava/lang/String;)V"),
    // picodroid/widget/SwipeRefreshLayout
    ("picodroid/widget/SwipeRefreshLayout", "nativeCreate", "()I"),
    ("picodroid/widget/SwipeRefreshLayout", "nativeRegisterRefreshListener", "()V"),
    ("picodroid/widget/SwipeRefreshLayout", "setRefreshing", "(Z)V"),
    // picodroid/widget/Switch
    ("picodroid/widget/Switch", "isChecked", "()Z"),
    ("picodroid/widget/Switch", "nativeCreate", "()I"),
    ("picodroid/widget/Switch", "setChecked", "(Z)V"),
    ("picodroid/widget/Switch", "toggle", "()V"),
    // picodroid/widget/TextView
    ("picodroid/widget/TextView", "getText", "()Ljava/lang/CharSequence;"),
    ("picodroid/widget/TextView", "nativeCreate", "()I"),
    ("picodroid/widget/TextView", "setIncludeFontPadding", "(Z)V"),
    ("picodroid/widget/TextView", "setText", "(Ljava/lang/String;)V"),
    ("picodroid/widget/TextView", "setTextColor", "(I)V"),
    // picodroid/widget/TimePicker
    ("picodroid/widget/TimePicker", "getHour", "()I"),
    ("picodroid/widget/TimePicker", "getMinute", "()I"),
    ("picodroid/widget/TimePicker", "is24HourView", "()Z"),
    ("picodroid/widget/TimePicker", "nativeCreate", "()I"),
    ("picodroid/widget/TimePicker", "nativeRegisterTimeChangedListener", "()V"),
    ("picodroid/widget/TimePicker", "setIs24HourView", "(Z)V"),
    ("picodroid/widget/TimePicker", "setTime", "(II)V"),
    // picodroid/widget/Toast
    ("picodroid/widget/Toast", "nativeCancel", "(I)V"),
    ("picodroid/widget/Toast", "nativeCreate", "(Ljava/lang/String;I)I"),
    ("picodroid/widget/Toast", "nativeSetDuration", "(II)V"),
    ("picodroid/widget/Toast", "nativeShow", "(I)V"),
    // picodroid/widget/ToggleButton
    ("picodroid/widget/ToggleButton", "isChecked", "()Z"),
    ("picodroid/widget/ToggleButton", "nativeCreate", "()I"),
    ("picodroid/widget/ToggleButton", "nativeCreateWithText", "(Ljava/lang/String;Ljava/lang/String;)I"),
    ("picodroid/widget/ToggleButton", "setChecked", "(Z)V"),
    ("picodroid/widget/ToggleButton", "setTextOff", "(Ljava/lang/String;)V"),
    ("picodroid/widget/ToggleButton", "setTextOn", "(Ljava/lang/String;)V"),
    ("picodroid/widget/ToggleButton", "toggle", "()V"),
];

pub const ALL_HANDLED: &[&[Row]] = &[
    PIO_HANDLED,
    IO_HANDLED,
    NET_HANDLED,
    OS_HANDLED,
    CONCURRENT_HANDLED,
    THREADS_HANDLED,
    SENSORS_HANDLED,
    APP_SERVICES_HANDLED,
    CORE_HANDLED,
    GRAPHICS_HANDLED,
];

// ── Migration state ─────────────────────────────────────────────────────────

/// Classes whose native methods are not yet migrated into a table above.
/// Machine-maintained: Direction A's failure output prints the exact rows
/// for any class removed from this list. Must reach `&[]` — every entry
/// hides potential silent `NoSuchMethod`s for that whole class.
pub const PENDING_CLASSES: &[&str] = &[];

/// Individual SDK `native` methods *intentionally* left unhandled on this
/// platform. Every entry is a deliberate runtime `NoSuchMethod` — justify
/// each in a comment (same discipline as `ALLOWED_UNREGISTERED` in
/// `class_registry.rs`).
pub const ALLOWED_UNHANDLED: &[Row] = &[];

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shrink_names::{unshrink_descriptor, unshrink_member};
    use pico_jvm::class_file::ClassFile;
    use std::collections::{BTreeMap, BTreeSet};

    /// JVMS §4.6 `ACC_NATIVE`.
    const ACC_NATIVE: u16 = 0x0100;

    fn sdk_native_methods() -> BTreeSet<(String, String, String)> {
        let mut sdk = BTreeSet::new();
        for bytes in crate::framework_classes::FRAMEWORK_CLASSES {
            let cf = ClassFile::parse(bytes).expect("parse framework class");
            let loaded = core::str::from_utf8(cf.class_name().expect("class name"))
                .expect("class name is UTF-8");
            let class = crate::shrink_names::unshrink_class(loaded).to_string();
            for m in cf.methods() {
                if m.access_flags & ACC_NATIVE == 0 {
                    continue;
                }
                let name = unshrink_member(
                    core::str::from_utf8(cf.cp_utf8(m.name_index).expect("method name utf8"))
                        .expect("method name is UTF-8"),
                )
                .to_string();
                let desc = core::str::from_utf8(
                    cf.cp_utf8(m.descriptor_index).expect("method descriptor"),
                )
                .expect("descriptor is UTF-8");
                sdk.insert((class.clone(), name, unshrink_descriptor(desc)));
            }
        }
        sdk
    }

    /// Both directions of the SDK ⇄ dispatch-table diff, plus duplicate-row
    /// rejection. One test so a failure shows the complete picture.
    #[test]
    fn every_native_method_is_handled_and_every_handler_entry_is_real() {
        let sdk = sdk_native_methods();

        // Non-vacuity: the SDK surface is ~308 ACC_NATIVE methods; a
        // collapse here means FRAMEWORK_CLASSES is empty/broken (e.g. a bare
        // `cargo test` without PICODROID_APK_PATH) and the diff is vacuous.
        assert!(
            sdk.len() >= 250,
            "only {} SDK native methods found — FRAMEWORK_CLASSES is empty or \
             the parser lost method attributes; this test is vacuous",
            sdk.len()
        );

        // Union the per-module tables, rejecting duplicates (a row may live
        // in exactly one module's table — see the CompoundButton note in the
        // module doc).
        let mut handled: BTreeSet<(String, String, String)> = BTreeSet::new();
        for table in ALL_HANDLED {
            for &(class, method, desc) in *table {
                let row = (class.to_string(), method.to_string(), desc.to_string());
                assert!(
                    handled.insert(row),
                    "duplicate handled row across tables: ({class}, {method}, {desc})"
                );
            }
        }
        // The JVM's rows are spelled as loaded; the platform tables above are
        // original names, so reverse-translate before unioning.
        for &(class, method, desc) in pico_jvm::native::BUILTIN_SDK_HANDLED {
            let row = (
                crate::shrink_names::unshrink_class(class).to_string(),
                unshrink_member(method).to_string(),
                unshrink_descriptor(desc),
            );
            assert!(
                handled.insert(row),
                "row duplicated between platform tables and BUILTIN_SDK_HANDLED: \
                 ({class}, {method}, {desc})"
            );
        }

        // Direction A: sdk − handled − allowlists == ∅.
        let mut missing: BTreeMap<String, Vec<(String, String)>> = BTreeMap::new();
        for (class, method, desc) in &sdk {
            if handled.contains(&(class.clone(), method.clone(), desc.clone()))
                || PENDING_CLASSES.contains(&class.as_str())
                || ALLOWED_UNHANDLED
                    .iter()
                    .any(|&(c, m, d)| c == class && m == method && d == desc)
            {
                continue;
            }
            missing
                .entry(class.clone())
                .or_default()
                .push((method.clone(), desc.clone()));
        }
        // Direction B: handled − sdk == ∅, with near-miss analysis.
        let mut bogus: Vec<String> = Vec::new();
        for (class, method, desc) in &handled {
            if sdk.contains(&(class.clone(), method.clone(), desc.clone())) {
                continue;
            }
            let overloads: Vec<&String> = sdk
                .iter()
                .filter(|(c, m, _)| c == class && m == method)
                .map(|(_, _, d)| d)
                .collect();
            if overloads.is_empty() {
                bogus.push(format!(
                    "({class}, {method}, {desc}) — no such native in the SDK: \
                     stale row, or the class/method was renamed"
                ));
            } else {
                bogus.push(format!(
                    "({class}, {method}, {desc}) — descriptor typo or overload \
                     drift; SDK declares: {overloads:?}"
                ));
            }
        }
        // Report both directions together: a wrong descriptor shows up in
        // *both* (the real one is unhandled, the typo'd one is bogus), and
        // seeing the pair side by side names the bug immediately.
        if !missing.is_empty() || !bogus.is_empty() {
            let mut msg = String::new();
            if !missing.is_empty() {
                msg.push_str(
                    "SDK native methods with no dispatch arm — each is a silent runtime \
                     NoSuchMethod. Paste the rows into the owning module's table in \
                     method_tables.rs (or the class into PENDING_CLASSES while \
                     migrating):\n",
                );
                for (class, rows) in &missing {
                    msg.push_str(&format!("\n    // {class}\n"));
                    for (method, desc) in rows {
                        msg.push_str(&format!("    ({class:?}, {method:?}, {desc:?}),\n"));
                    }
                }
            }
            if !bogus.is_empty() {
                msg.push_str("\nhandled rows with no matching SDK native declaration:\n  ");
                msg.push_str(&bogus.join("\n  "));
                msg.push('\n');
            }
            panic!("{msg}");
        }

        // A PENDING_CLASSES entry for a class with no natives is stale.
        for class in PENDING_CLASSES {
            assert!(
                sdk.iter().any(|(c, _, _)| c == class),
                "PENDING_CLASSES entry {class:?} matches no SDK class with \
                 native methods — remove it"
            );
        }
    }
}
