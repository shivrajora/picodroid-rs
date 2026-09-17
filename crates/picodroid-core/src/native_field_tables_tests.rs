// SPDX-License-Identifier: GPL-3.0-only
//! The hand-numbered native field tables against the class files they
//! mirror.
//!
//! Native code reaches into SDK objects by field *slot*: `graphics/fields.rs`,
//! `pio/fields.rs`, `net/fields.rs`, the `fields` modules of
//! `hardware/sensors` and `native_handler/io`. Those numbers are written by
//! hand from the Java declaration order, and since a `long` or `double`
//! field takes two slots (JVMS §2.6.1; `Value` 16 B → 8 B, 2026-09-16) every
//! constant after one is off by one unless someone remembers. Nothing else
//! would notice: a wrong slot reads the other half of a `long` as `None` or
//! writes over it. This test resolves each named field through the JVM's own
//! `field_slot` over the embedded framework classes — the same walk the
//! interpreter's `getfield` uses — and compares.
//!
//! Runs under both shrink modes (`scripts/test.sh`): class constants are
//! spelled as loaded and field names go through `shrink_member`.

use crate::shrink_names::{c, shrink_member};
use pico_jvm::class_file::ClassFile;
use pico_jvm::interpreter::{field_slot, instance_slot_count};

fn framework_classes() -> Vec<ClassFile> {
    let classes: Vec<ClassFile> = crate::framework_classes::FRAMEWORK_CLASSES
        .iter()
        .map(|b| ClassFile::parse(b).expect("parse framework class"))
        .collect();
    assert!(
        !classes.is_empty(),
        "FRAMEWORK_CLASSES is empty — run via scripts/test.sh, which sets PICODROID_APK_PATH"
    );
    classes
}

/// `(loaded class name, original field name, the table's slot)`.
fn slot_expectations() -> Vec<(&'static str, &'static str, usize)> {
    use crate::graphics_fields_tests as g;
    use crate::hardware::sensors::{event_fields, fields as sensor};
    use crate::native_handler_io_tests::fields as io;
    use crate::net_fields_tests as net;
    use crate::pio::fields as pio;
    vec![
        // graphics/fields.rs
        (
            c::picodroid_view_View,
            "nativeHandle",
            g::view::NATIVE_HANDLE,
        ),
        (c::picodroid_graphics_Display, "width", g::display::WIDTH),
        (c::picodroid_graphics_Display, "height", g::display::HEIGHT),
        (
            c::picodroid_view_MotionEvent,
            "action",
            g::motion_event::ACTION,
        ),
        (c::picodroid_view_MotionEvent, "x", g::motion_event::X),
        (c::picodroid_view_MotionEvent, "y", g::motion_event::Y),
        (
            c::picodroid_view_MotionEvent,
            "eventTime",
            g::motion_event::EVENT_TIME,
        ),
        (
            c::picodroid_view_MotionEvent,
            "rawX",
            g::motion_event::RAW_X,
        ),
        (
            c::picodroid_view_MotionEvent,
            "rawY",
            g::motion_event::RAW_Y,
        ),
        (c::picodroid_view_KeyEvent, "action", g::key_event::ACTION),
        (
            c::picodroid_view_KeyEvent,
            "keyCode",
            g::key_event::KEY_CODE,
        ),
        (
            c::picodroid_app_AlertDialog,
            "nativeHandle",
            g::alert_dialog::NATIVE_HANDLE,
        ),
        (
            c::picodroid_widget_Snackbar,
            "nativeHandle",
            g::snackbar::NATIVE_HANDLE,
        ),
        (
            c::picodroid_content_Intent,
            "targetClassName",
            g::intent::TARGET_CLASS_NAME,
        ),
        (
            c::picodroid_content_Intent,
            "packageName",
            g::intent::PACKAGE,
        ),
        // pio/fields.rs — slot 0 of each; the rest is native-only state.
        (c::picodroid_pio_Gpio, "pin", pio::gpio::PIN),
        (c::picodroid_pio_UartDevice, "uartId", pio::uart::UART_ID),
        (c::picodroid_pio_I2cDevice, "i2cId", pio::i2c::I2C_ID),
        (c::picodroid_pio_SpiDevice, "spiId", pio::spi::SPI_ID),
        (c::picodroid_pio_Pwm, "pin", pio::pwm::PIN),
        (c::picodroid_pio_Adc, "pin", pio::adc::PIN),
        // hardware/sensors/mod.rs
        (c::picodroid_hardware_Sensor, "type", sensor::TYPE),
        (c::picodroid_hardware_Sensor, "name", sensor::NAME),
        (c::picodroid_hardware_Sensor, "vendor", sensor::VENDOR),
        (c::picodroid_hardware_Sensor, "maxRange", sensor::MAX_RANGE),
        (
            c::picodroid_hardware_Sensor,
            "resolution",
            sensor::RESOLUTION,
        ),
        (c::picodroid_hardware_Sensor, "minDelay", sensor::MIN_DELAY),
        (
            c::picodroid_hardware_SensorEvent,
            "sensor",
            event_fields::SENSOR,
        ),
        (
            c::picodroid_hardware_SensorEvent,
            "values",
            event_fields::VALUES,
        ),
        (
            c::picodroid_hardware_SensorEvent,
            "accuracy",
            event_fields::ACCURACY,
        ),
        (
            c::picodroid_hardware_SensorEvent,
            "timestamp",
            event_fields::TIMESTAMP,
        ),
        // native_handler/io/mod.rs
        (c::picodroid_io_File, "path", io::file::PATH),
        (c::picodroid_io_FileInputStream, "path", io::fis::PATH),
        (c::picodroid_io_FileInputStream, "pos", io::fis::POS),
        (c::picodroid_io_FileOutputStream, "path", io::fos::PATH),
        (c::picodroid_io_FileOutputStream, "pos", io::fos::POS),
        // net/fields.rs
        (c::picodroid_net_Socket, "handle", net::socket::HANDLE),
        (
            c::picodroid_net_ServerSocket,
            "handle",
            net::server_socket::HANDLE,
        ),
        (
            c::picodroid_net_DatagramSocket,
            "handle",
            net::datagram_socket::HANDLE,
        ),
        (
            c::picodroid_net_DatagramPacket,
            "data",
            net::datagram_packet::DATA,
        ),
        (
            c::picodroid_net_DatagramPacket,
            "length",
            net::datagram_packet::LENGTH,
        ),
        (
            c::picodroid_net_DatagramPacket,
            "address",
            net::datagram_packet::ADDRESS,
        ),
        (
            c::picodroid_net_DatagramPacket,
            "port",
            net::datagram_packet::PORT,
        ),
        (
            c::picodroid_net_HttpInputStream,
            "handle",
            net::http_input_stream::HANDLE,
        ),
        (
            c::picodroid_net_HttpOutputStream,
            "handle",
            net::http_output_stream::HANDLE,
        ),
    ]
}

#[test]
fn native_field_tables_match_the_class_files() {
    let classes = framework_classes();
    let mut wrong = Vec::new();
    for (class, field, expected) in slot_expectations() {
        let actual = field_slot(&classes, class, shrink_member(field));
        if actual != Some(expected) {
            wrong.push(format!(
                "{class}.{field}: table says slot {expected}, class file says {actual:?}"
            ));
        }
    }
    assert!(
        wrong.is_empty(),
        "native field tables drifted from the class files (a long/double field takes two slots):\n  {}",
        wrong.join("\n  ")
    );
}

/// The widths native code allocates recycled instances with: a `long` at
/// the end of the class means one more slot than its index plus one.
#[test]
fn native_alloc_widths_match_the_class_files() {
    use crate::graphics_fields_tests::{key_event, motion_event};
    use crate::hardware::sensors::event_fields;
    let classes = framework_classes();
    for (class, slots) in [
        (c::picodroid_view_MotionEvent, motion_event::SLOTS),
        (c::picodroid_view_KeyEvent, key_event::SLOTS),
        (c::picodroid_hardware_SensorEvent, event_fields::SLOTS),
    ] {
        assert_eq!(
            instance_slot_count(&classes, class),
            Some(slots),
            "{class}: the native allocation width does not match the class file"
        );
    }
}
