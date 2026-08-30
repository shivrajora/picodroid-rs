// SPDX-License-Identifier: GPL-3.0-only
@file:JvmName("SntpClient")

package picoenvmonkt.net

import java.io.IOException
import picodroid.net.DatagramPacket
import picodroid.net.DatagramSocket
import picodroid.net.InetAddress
import picodroid.os.SystemClock
import picodroid.util.Log
import picoenvmonkt.TAG

/**
 * Minimal SNTP (RFC 4330) client. Android hides its SntpClient as internal API, so this lives in
 * app code. One 48-byte exchange: mode 3 (client) request, read the server's transmit timestamp
 * (seconds since 1900 at offset 40), convert to Unix epoch millis, and anchor the wall clock via
 * [SystemClock.setCurrentTimeMillis]. No round-trip-delay compensation — display accuracy on a
 * sensor monitor doesn't need sub-100ms truth.
 */
private const val NTP_HOST = "pool.ntp.org"
private const val NTP_PORT = 123
private const val PACKET_BYTES = 48
private const val TIMEOUT_MS = 3000
private const val ATTEMPTS = 3

/** Seconds between the NTP era (1900-01-01) and the Unix epoch (1970-01-01). */
private const val SECONDS_1900_TO_1970 = 2208988800L

/**
 * Resolve, exchange, and anchor the clock. Returns true on success. Fail-soft: every failure is
 * caught and logged — callers retry from the housekeeping tick.
 */
fun sntpSync(): Boolean {
    var socket: DatagramSocket? = null
    try {
        val server = InetAddress.getByName(NTP_HOST).rawAddress
        val s = DatagramSocket(0)
        socket = s
        s.setTimeout(TIMEOUT_MS)
        for (attempt in 1..ATTEMPTS) {
            val epochMs = exchange(s, server)
            if (epochMs > 0) {
                SystemClock.setCurrentTimeMillis(epochMs)
                Log.i(TAG, "ntp: synced, epoch=$epochMs")
                return true
            }
        }
        Log.i(TAG, "ntp: no valid reply after $ATTEMPTS attempts")
        return false
    } catch (e: IOException) {
        Log.i(TAG, "ntp: sync failed: ${e.message}")
        return false
    } catch (e: RuntimeException) {
        Log.i(TAG, "ntp: unexpected: $e")
        return false
    } finally {
        socket?.close()
    }
}

/** One request/reply. Returns epoch millis, or 0 on timeout/garbage. */
private fun exchange(socket: DatagramSocket, server: Int): Long {
    try {
        val buf = ByteArray(PACKET_BYTES)
        // LI=0, VN=4, Mode=3 (client).
        buf[0] = 0x23
        socket.send(DatagramPacket(buf, PACKET_BYTES, server, NTP_PORT))

        val reply = DatagramPacket(buf, PACKET_BYTES)
        socket.receive(reply)
        if (reply.length < PACKET_BYTES) {
            return 0
        }
        // Transmit timestamp: 32-bit unsigned seconds at offset 40, then the
        // 32-bit fraction — take the top byte for ~4 ms granularity.
        val ntpSeconds =
            ((buf[40].toLong() and 0xFFL) shl 24) or
                ((buf[41].toLong() and 0xFFL) shl 16) or
                ((buf[42].toLong() and 0xFFL) shl 8) or
                (buf[43].toLong() and 0xFFL)
        if (ntpSeconds == 0L) {
            return 0
        }
        val fractionMs = ((buf[44].toLong() and 0xFFL) * 1000L) shr 8
        return (ntpSeconds - SECONDS_1900_TO_1970) * 1000L + fractionMs
    } catch (e: IOException) {
        Log.i(TAG, "ntp: attempt failed: ${e.message}")
        return 0
    }
}
