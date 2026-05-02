// SPDX-License-Identifier: GPL-3.0-only
//! Native method dispatch for picodroid.net.* classes.

use pico_jvm::types::{JvmError, Value};
use pico_jvm::NativeContext;

pub fn dispatch(
    class_name: &str,
    method_name: &str,
    ctx: &mut NativeContext<'_>,
) -> Option<Result<Option<Value>, JvmError>> {
    let class_name = crate::shrink_names::unshrink_class(class_name);
    match (class_name, method_name) {
        // ── Socket ──────────────────────────────────────────────────────
        ("picodroid/net/Socket", "nativeCreate") => {
            Some(crate::system::picodroid::net::socket::native_create())
        }
        ("picodroid/net/Socket", "connect") => Some(
            crate::system::picodroid::net::socket::connect_native(ctx.args, ctx.objects),
        ),
        ("picodroid/net/Socket", "send") => Some(
            crate::system::picodroid::net::socket::send_native(ctx.args, ctx.objects, ctx.arrays),
        ),
        ("picodroid/net/Socket", "recv") => Some(
            crate::system::picodroid::net::socket::recv_native(ctx.args, ctx.objects, ctx.arrays),
        ),
        ("picodroid/net/Socket", "setTimeout") => Some(
            crate::system::picodroid::net::socket::set_timeout_native(ctx.args, ctx.objects),
        ),
        ("picodroid/net/Socket", "close") => Some(
            crate::system::picodroid::net::socket::close_native(ctx.args, ctx.objects),
        ),

        // ── ServerSocket ────────────────────────────────────────────────
        ("picodroid/net/ServerSocket", "nativeListen") => Some(
            crate::system::picodroid::net::server_socket::native_listen(ctx.args),
        ),
        ("picodroid/net/ServerSocket", "accept") => Some(
            crate::system::picodroid::net::server_socket::accept_native(ctx.args, ctx.objects),
        ),
        ("picodroid/net/ServerSocket", "close") => Some(
            crate::system::picodroid::net::server_socket::close_native(ctx.args, ctx.objects),
        ),

        // ── DatagramSocket ──────────────────────────────────────────────
        ("picodroid/net/DatagramSocket", "nativeCreate") => {
            Some(crate::system::picodroid::net::datagram_socket::native_create(ctx.args))
        }
        ("picodroid/net/DatagramSocket", "send") => {
            Some(crate::system::picodroid::net::datagram_socket::send_native(
                ctx.args,
                ctx.objects,
                ctx.arrays,
            ))
        }
        ("picodroid/net/DatagramSocket", "receive") => Some(
            crate::system::picodroid::net::datagram_socket::receive_native(
                ctx.args,
                ctx.objects,
                ctx.arrays,
            ),
        ),
        ("picodroid/net/DatagramSocket", "setTimeout") => Some(
            crate::system::picodroid::net::datagram_socket::set_timeout_native(
                ctx.args,
                ctx.objects,
            ),
        ),
        ("picodroid/net/DatagramSocket", "close") => Some(
            crate::system::picodroid::net::datagram_socket::close_native(ctx.args, ctx.objects),
        ),

        // ── InetAddress ──────────────────────────────────────────────────
        ("picodroid/net/InetAddress", "getHostAddress") => Some(
            crate::system::picodroid::net::inet_address::get_host_address_native(
                ctx.args,
                ctx.objects,
                ctx.strings,
            ),
        ),

        // ── NetworkInfo ─────────────────────────────────────────────────
        ("picodroid/net/NetworkInfo", "isConnected") => {
            Some(crate::system::picodroid::net::network_info::is_connected_native())
        }
        ("picodroid/net/NetworkInfo", "getIpAddress") => {
            Some(crate::system::picodroid::net::network_info::get_ip_address_native())
        }

        // ── HttpUrlConnection ───────────────────────────────────────────
        ("picodroid/net/HttpUrlConnection", "nativeConnect") => Some(
            crate::system::picodroid::net::http_connection::native_connect(ctx.args, ctx.strings),
        ),
        ("picodroid/net/HttpUrlConnection", "nativeReadResponseCode") => Some(
            crate::system::picodroid::net::http_connection::native_read_response_code(ctx.args),
        ),
        ("picodroid/net/HttpUrlConnection", "nativeContentLength") => {
            Some(crate::system::picodroid::net::http_connection::native_content_length(ctx.args))
        }
        ("picodroid/net/HttpUrlConnection", "nativeDisconnect") => {
            Some(crate::system::picodroid::net::http_connection::native_disconnect(ctx.args))
        }

        // ── HttpInputStream ─────────────────────────────────────────────
        ("picodroid/net/HttpInputStream", "read") => Some(
            crate::system::picodroid::net::http_connection::native_input_read(
                ctx.args,
                ctx.objects,
                ctx.arrays,
            ),
        ),

        // ── HttpOutputStream ────────────────────────────────────────────
        ("picodroid/net/HttpOutputStream", "write") => Some(
            crate::system::picodroid::net::http_connection::native_output_write(
                ctx.args,
                ctx.objects,
                ctx.arrays,
            ),
        ),

        _ => None,
    }
}
