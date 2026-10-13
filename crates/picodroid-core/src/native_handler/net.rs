// SPDX-License-Identifier: GPL-3.0-only
//! Native method dispatch for picodroid.net.* classes.
use crate::shrink_names::{ch, mh};
use pico_jvm::NativeKey;

use pico_jvm::types::{JvmError, Value};
use pico_jvm::NativeContext;

pub fn dispatch(
    key: NativeKey,
    ctx: &mut NativeContext<'_>,
) -> Option<Result<Option<Value>, JvmError>> {
    match (key.class, key.method) {
        // ── Socket ──────────────────────────────────────────────────────
        (ch::picodroid_net_Socket, mh::nativeCreate) => {
            Some(crate::net::socket::native_create(ctx.objects, ctx.strings))
        }
        (ch::picodroid_net_Socket, mh::connect) => Some(crate::net::socket::connect_native(
            ctx.args,
            ctx.objects,
            ctx.strings,
        )),
        (ch::picodroid_net_Socket, mh::send) => Some(crate::net::socket::send_native(
            ctx.args,
            ctx.objects,
            ctx.strings,
            ctx.arrays,
        )),
        (ch::picodroid_net_Socket, mh::recv) => Some(crate::net::socket::recv_native(
            ctx.args,
            ctx.objects,
            ctx.strings,
            ctx.arrays,
        )),
        (ch::picodroid_net_Socket, mh::setTimeout) => Some(crate::net::socket::set_timeout_native(
            ctx.args,
            ctx.objects,
            ctx.strings,
        )),
        (ch::picodroid_net_Socket, mh::close) => {
            Some(crate::net::socket::close_native(ctx.args, ctx.objects))
        }

        // ── ServerSocket ────────────────────────────────────────────────
        (ch::picodroid_net_ServerSocket, mh::nativeListen) => Some(
            crate::net::server_socket::native_listen(ctx.args, ctx.objects, ctx.strings),
        ),
        (ch::picodroid_net_ServerSocket, mh::accept) => Some(
            crate::net::server_socket::accept_native(ctx.args, ctx.objects, ctx.strings),
        ),
        (ch::picodroid_net_ServerSocket, mh::setSoTimeout) => Some(
            crate::net::server_socket::set_so_timeout_native(ctx.args, ctx.objects, ctx.strings),
        ),
        (ch::picodroid_net_ServerSocket, mh::close) => Some(
            crate::net::server_socket::close_native(ctx.args, ctx.objects),
        ),

        // ── DatagramSocket ──────────────────────────────────────────────
        (ch::picodroid_net_DatagramSocket, mh::nativeCreate) => Some(
            crate::net::datagram_socket::native_create(ctx.args, ctx.objects, ctx.strings),
        ),
        (ch::picodroid_net_DatagramSocket, mh::send) => {
            Some(crate::net::datagram_socket::send_native(
                ctx.args,
                ctx.objects,
                ctx.strings,
                ctx.arrays,
            ))
        }
        (ch::picodroid_net_DatagramSocket, mh::receive) => {
            Some(crate::net::datagram_socket::receive_native(
                ctx.args,
                ctx.objects,
                ctx.strings,
                ctx.arrays,
            ))
        }
        (ch::picodroid_net_DatagramSocket, mh::setTimeout) => Some(
            crate::net::datagram_socket::set_timeout_native(ctx.args, ctx.objects, ctx.strings),
        ),
        (ch::picodroid_net_DatagramSocket, mh::close) => Some(
            crate::net::datagram_socket::close_native(ctx.args, ctx.objects),
        ),

        // ── InetAddress ──────────────────────────────────────────────────
        (ch::picodroid_net_InetAddress, mh::getHostAddress) => Some(
            crate::net::inet_address::get_host_address_native(ctx.args, ctx.objects, ctx.strings),
        ),
        (ch::picodroid_net_InetAddress, mh::nativeResolve) => Some(
            crate::net::inet_address::native_resolve(ctx.args, ctx.objects, ctx.strings),
        ),

        // ── NetworkInfo ─────────────────────────────────────────────────
        (ch::picodroid_net_NetworkInfo, mh::isConnected) => {
            Some(crate::net::network_info::is_connected_native())
        }
        (ch::picodroid_net_NetworkInfo, mh::getIpAddress) => {
            Some(crate::net::network_info::get_ip_address_native())
        }
        (ch::picodroid_net_NetworkInfo, mh::getType) => {
            Some(crate::net::network_info::get_type_native())
        }

        // ── HttpURLConnection ───────────────────────────────────────────
        (ch::picodroid_net_HttpURLConnection, mh::nativeConnect) => Some(
            crate::net::http_connection::native_connect(ctx.args, ctx.objects, ctx.strings),
        ),
        (ch::picodroid_net_HttpURLConnection, mh::nativeReadResponseCode) => {
            Some(crate::net::http_connection::native_read_response_code(
                ctx.args,
                ctx.objects,
                ctx.strings,
            ))
        }
        (ch::picodroid_net_HttpURLConnection, mh::nativeContentLength) => Some(
            crate::net::http_connection::native_content_length(ctx.args, ctx.objects, ctx.strings),
        ),
        (ch::picodroid_net_HttpURLConnection, mh::nativeHeaderField) => Some(
            crate::net::http_connection::native_header_field(ctx.args, ctx.objects, ctx.strings),
        ),
        (ch::picodroid_net_HttpURLConnection, mh::nativeHeaderFieldAt) => Some(
            crate::net::http_connection::native_header_field_at(ctx.args, ctx.objects, ctx.strings),
        ),
        (ch::picodroid_net_HttpURLConnection, mh::nativeResponseMessage) => {
            Some(crate::net::http_connection::native_response_message(
                ctx.args,
                ctx.objects,
                ctx.strings,
            ))
        }
        (ch::picodroid_net_HttpURLConnection, mh::nativeDisconnect) => {
            Some(crate::net::http_connection::native_disconnect(ctx.args))
        }

        // ── HttpInputStream ─────────────────────────────────────────────
        (ch::picodroid_net_HttpInputStream, mh::read) => {
            Some(crate::net::http_connection::native_input_read(
                ctx.args,
                ctx.objects,
                ctx.strings,
                ctx.arrays,
            ))
        }

        // ── HttpOutputStream ────────────────────────────────────────────
        (ch::picodroid_net_HttpOutputStream, mh::write) => {
            Some(crate::net::http_connection::native_output_write(
                ctx.args,
                ctx.objects,
                ctx.strings,
                ctx.arrays,
            ))
        }

        // ── WifiManager ─────────────────────────────────────────────────
        #[cfg(network_link_wifi)]
        (ch::picodroid_net_wifi_WifiManager, _) => {
            crate::net::wifi_manager::dispatch(key.method, ctx)
        }
        #[cfg(not(network_link_wifi))]
        (ch::picodroid_net_wifi_WifiManager, _) => super::wifi_stub::dispatch(key.method, ctx),

        _ => None,
    }
}
