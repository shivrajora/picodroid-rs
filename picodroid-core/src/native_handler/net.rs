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
            Some(crate::net::socket::native_create(ctx.objects, ctx.strings))
        }
        ("picodroid/net/Socket", "connect") => Some(crate::net::socket::connect_native(
            ctx.args,
            ctx.objects,
            ctx.strings,
        )),
        ("picodroid/net/Socket", "send") => Some(crate::net::socket::send_native(
            ctx.args,
            ctx.objects,
            ctx.strings,
            ctx.arrays,
        )),
        ("picodroid/net/Socket", "recv") => Some(crate::net::socket::recv_native(
            ctx.args,
            ctx.objects,
            ctx.strings,
            ctx.arrays,
        )),
        ("picodroid/net/Socket", "setTimeout") => Some(crate::net::socket::set_timeout_native(
            ctx.args,
            ctx.objects,
            ctx.strings,
        )),
        ("picodroid/net/Socket", "close") => {
            Some(crate::net::socket::close_native(ctx.args, ctx.objects))
        }

        // ── ServerSocket ────────────────────────────────────────────────
        ("picodroid/net/ServerSocket", "nativeListen") => Some(
            crate::net::server_socket::native_listen(ctx.args, ctx.objects, ctx.strings),
        ),
        ("picodroid/net/ServerSocket", "accept") => Some(crate::net::server_socket::accept_native(
            ctx.args,
            ctx.objects,
            ctx.strings,
        )),
        ("picodroid/net/ServerSocket", "setSoTimeout") => Some(
            crate::net::server_socket::set_so_timeout_native(ctx.args, ctx.objects, ctx.strings),
        ),
        ("picodroid/net/ServerSocket", "close") => Some(crate::net::server_socket::close_native(
            ctx.args,
            ctx.objects,
        )),

        // ── DatagramSocket ──────────────────────────────────────────────
        ("picodroid/net/DatagramSocket", "nativeCreate") => Some(
            crate::net::datagram_socket::native_create(ctx.args, ctx.objects, ctx.strings),
        ),
        ("picodroid/net/DatagramSocket", "send") => Some(crate::net::datagram_socket::send_native(
            ctx.args,
            ctx.objects,
            ctx.strings,
            ctx.arrays,
        )),
        ("picodroid/net/DatagramSocket", "receive") => {
            Some(crate::net::datagram_socket::receive_native(
                ctx.args,
                ctx.objects,
                ctx.strings,
                ctx.arrays,
            ))
        }
        ("picodroid/net/DatagramSocket", "setTimeout") => Some(
            crate::net::datagram_socket::set_timeout_native(ctx.args, ctx.objects, ctx.strings),
        ),
        ("picodroid/net/DatagramSocket", "close") => Some(
            crate::net::datagram_socket::close_native(ctx.args, ctx.objects),
        ),

        // ── InetAddress ──────────────────────────────────────────────────
        ("picodroid/net/InetAddress", "getHostAddress") => Some(
            crate::net::inet_address::get_host_address_native(ctx.args, ctx.objects, ctx.strings),
        ),
        ("picodroid/net/InetAddress", "nativeResolve") => Some(
            crate::net::inet_address::native_resolve(ctx.args, ctx.objects, ctx.strings),
        ),

        // ── NetworkInfo ─────────────────────────────────────────────────
        ("picodroid/net/NetworkInfo", "isConnected") => {
            Some(crate::net::network_info::is_connected_native())
        }
        ("picodroid/net/NetworkInfo", "getIpAddress") => {
            Some(crate::net::network_info::get_ip_address_native())
        }

        // ── HttpURLConnection ───────────────────────────────────────────
        ("picodroid/net/HttpURLConnection", "nativeConnect") => Some(
            crate::net::http_connection::native_connect(ctx.args, ctx.objects, ctx.strings),
        ),
        ("picodroid/net/HttpURLConnection", "nativeReadResponseCode") => {
            Some(crate::net::http_connection::native_read_response_code(
                ctx.args,
                ctx.objects,
                ctx.strings,
            ))
        }
        ("picodroid/net/HttpURLConnection", "nativeContentLength") => Some(
            crate::net::http_connection::native_content_length(ctx.args, ctx.objects, ctx.strings),
        ),
        ("picodroid/net/HttpURLConnection", "nativeHeaderField") => Some(
            crate::net::http_connection::native_header_field(ctx.args, ctx.objects, ctx.strings),
        ),
        ("picodroid/net/HttpURLConnection", "nativeHeaderFieldAt") => Some(
            crate::net::http_connection::native_header_field_at(ctx.args, ctx.objects, ctx.strings),
        ),
        ("picodroid/net/HttpURLConnection", "nativeResponseMessage") => {
            Some(crate::net::http_connection::native_response_message(
                ctx.args,
                ctx.objects,
                ctx.strings,
            ))
        }
        ("picodroid/net/HttpURLConnection", "nativeDisconnect") => {
            Some(crate::net::http_connection::native_disconnect(ctx.args))
        }

        // ── HttpInputStream ─────────────────────────────────────────────
        ("picodroid/net/HttpInputStream", "read") => {
            Some(crate::net::http_connection::native_input_read(
                ctx.args,
                ctx.objects,
                ctx.strings,
                ctx.arrays,
            ))
        }

        // ── HttpOutputStream ────────────────────────────────────────────
        ("picodroid/net/HttpOutputStream", "write") => {
            Some(crate::net::http_connection::native_output_write(
                ctx.args,
                ctx.objects,
                ctx.strings,
                ctx.arrays,
            ))
        }

        _ => None,
    }
}
