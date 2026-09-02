// SPDX-License-Identifier: GPL-3.0-only
//! Native method dispatch for picodroid.net.* classes.

use crate::shrink_names::c;
use crate::shrink_names::m;
use pico_jvm::types::{JvmError, Value};
use pico_jvm::NativeContext;

pub fn dispatch(
    class_name: &str,
    method_name: &str,
    ctx: &mut NativeContext<'_>,
) -> Option<Result<Option<Value>, JvmError>> {
    match (class_name, method_name) {
        // ── Socket ──────────────────────────────────────────────────────
        (c::picodroid_net_Socket, m::nativeCreate) => {
            Some(crate::net::socket::native_create(ctx.objects, ctx.strings))
        }
        (c::picodroid_net_Socket, m::connect) => Some(crate::net::socket::connect_native(
            ctx.args,
            ctx.objects,
            ctx.strings,
        )),
        (c::picodroid_net_Socket, m::send) => Some(crate::net::socket::send_native(
            ctx.args,
            ctx.objects,
            ctx.strings,
            ctx.arrays,
        )),
        (c::picodroid_net_Socket, m::recv) => Some(crate::net::socket::recv_native(
            ctx.args,
            ctx.objects,
            ctx.strings,
            ctx.arrays,
        )),
        (c::picodroid_net_Socket, m::setTimeout) => Some(crate::net::socket::set_timeout_native(
            ctx.args,
            ctx.objects,
            ctx.strings,
        )),
        (c::picodroid_net_Socket, m::close) => {
            Some(crate::net::socket::close_native(ctx.args, ctx.objects))
        }

        // ── ServerSocket ────────────────────────────────────────────────
        (c::picodroid_net_ServerSocket, m::nativeListen) => Some(
            crate::net::server_socket::native_listen(ctx.args, ctx.objects, ctx.strings),
        ),
        (c::picodroid_net_ServerSocket, m::accept) => Some(
            crate::net::server_socket::accept_native(ctx.args, ctx.objects, ctx.strings),
        ),
        (c::picodroid_net_ServerSocket, m::setSoTimeout) => Some(
            crate::net::server_socket::set_so_timeout_native(ctx.args, ctx.objects, ctx.strings),
        ),
        (c::picodroid_net_ServerSocket, m::close) => Some(crate::net::server_socket::close_native(
            ctx.args,
            ctx.objects,
        )),

        // ── DatagramSocket ──────────────────────────────────────────────
        (c::picodroid_net_DatagramSocket, m::nativeCreate) => Some(
            crate::net::datagram_socket::native_create(ctx.args, ctx.objects, ctx.strings),
        ),
        (c::picodroid_net_DatagramSocket, m::send) => {
            Some(crate::net::datagram_socket::send_native(
                ctx.args,
                ctx.objects,
                ctx.strings,
                ctx.arrays,
            ))
        }
        (c::picodroid_net_DatagramSocket, m::receive) => {
            Some(crate::net::datagram_socket::receive_native(
                ctx.args,
                ctx.objects,
                ctx.strings,
                ctx.arrays,
            ))
        }
        (c::picodroid_net_DatagramSocket, m::setTimeout) => Some(
            crate::net::datagram_socket::set_timeout_native(ctx.args, ctx.objects, ctx.strings),
        ),
        (c::picodroid_net_DatagramSocket, m::close) => Some(
            crate::net::datagram_socket::close_native(ctx.args, ctx.objects),
        ),

        // ── InetAddress ──────────────────────────────────────────────────
        (c::picodroid_net_InetAddress, m::getHostAddress) => Some(
            crate::net::inet_address::get_host_address_native(ctx.args, ctx.objects, ctx.strings),
        ),
        (c::picodroid_net_InetAddress, m::nativeResolve) => Some(
            crate::net::inet_address::native_resolve(ctx.args, ctx.objects, ctx.strings),
        ),

        // ── NetworkInfo ─────────────────────────────────────────────────
        (c::picodroid_net_NetworkInfo, m::isConnected) => {
            Some(crate::net::network_info::is_connected_native())
        }
        (c::picodroid_net_NetworkInfo, m::getIpAddress) => {
            Some(crate::net::network_info::get_ip_address_native())
        }

        // ── HttpURLConnection ───────────────────────────────────────────
        (c::picodroid_net_HttpURLConnection, m::nativeConnect) => Some(
            crate::net::http_connection::native_connect(ctx.args, ctx.objects, ctx.strings),
        ),
        (c::picodroid_net_HttpURLConnection, m::nativeReadResponseCode) => {
            Some(crate::net::http_connection::native_read_response_code(
                ctx.args,
                ctx.objects,
                ctx.strings,
            ))
        }
        (c::picodroid_net_HttpURLConnection, m::nativeContentLength) => Some(
            crate::net::http_connection::native_content_length(ctx.args, ctx.objects, ctx.strings),
        ),
        (c::picodroid_net_HttpURLConnection, m::nativeHeaderField) => Some(
            crate::net::http_connection::native_header_field(ctx.args, ctx.objects, ctx.strings),
        ),
        (c::picodroid_net_HttpURLConnection, m::nativeHeaderFieldAt) => Some(
            crate::net::http_connection::native_header_field_at(ctx.args, ctx.objects, ctx.strings),
        ),
        (c::picodroid_net_HttpURLConnection, m::nativeResponseMessage) => {
            Some(crate::net::http_connection::native_response_message(
                ctx.args,
                ctx.objects,
                ctx.strings,
            ))
        }
        (c::picodroid_net_HttpURLConnection, m::nativeDisconnect) => {
            Some(crate::net::http_connection::native_disconnect(ctx.args))
        }

        // ── HttpInputStream ─────────────────────────────────────────────
        (c::picodroid_net_HttpInputStream, m::read) => {
            Some(crate::net::http_connection::native_input_read(
                ctx.args,
                ctx.objects,
                ctx.strings,
                ctx.arrays,
            ))
        }

        // ── HttpOutputStream ────────────────────────────────────────────
        (c::picodroid_net_HttpOutputStream, m::write) => {
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
