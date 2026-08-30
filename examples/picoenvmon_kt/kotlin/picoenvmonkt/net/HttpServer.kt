// SPDX-License-Identifier: GPL-3.0-only
package picoenvmonkt.net

import java.io.IOException
import java.net.SocketTimeoutException
import picodroid.net.ServerSocket
import picodroid.net.Socket
import picodroid.os.SystemClock
import picodroid.util.Log
import picoenvmonkt.CITY
import picoenvmonkt.HTTP_PORT
import picoenvmonkt.IDX_GAS
import picoenvmonkt.IDX_HUMIDITY
import picoenvmonkt.IDX_LIGHT
import picoenvmonkt.IDX_PRESSURE
import picoenvmonkt.IDX_TEMPERATURE
import picoenvmonkt.TAG
import picoenvmonkt.data.LatestReadings
import picoenvmonkt.util.Formatter
import picoenvmonkt.util.UTC_OFFSET_MINUTES

private const val ACCEPT_TIMEOUT_MS = 1000
private const val CLIENT_TIMEOUT_MS = 2000
private const val REQUEST_BUF_BYTES = 512
private const val PAGE_BUF_BYTES = 1536

/**
 * Minimal HTTP/1.0 dashboard server, driven from the NetworkManager thread. One connection at a
 * time by design: the native listen backlog is 1, the page is ~1 KB, and the browser's 2 s
 * meta-refresh keeps concurrency at ~1 — serial serving is the architecture, not a shortcut. The 1
 * s accept timeout is the caller's housekeeping tick.
 *
 * Every per-connection failure is caught and logged: this thread is the app's entire network stack
 * and must never die (a dead JvmChild task does not respawn on device).
 *
 * The constant page fragments are instance fields, not statics: exactly one HttpServer ever exists
 * (NetworkManager.runOnline), so this is the same storage as Java's `static final byte[]`s with no
 * holder class — and no `toByteArray()`, which inlines to a `Charset` overload pico-jvm lacks.
 */
class HttpServer(
    private val latestReadings: LatestReadings,
    private val formatter: Formatter,
    private val net: NetworkManager,
) {
    // Constant page framing, cached as bytes once — the page rebuilds every 2 s
    // forever, so per-request churn matters (GC pacing is a known sore point in
    // this app). Dark palette matches the on-device theme.
    private val pageHead =
        formatter.ascii(
            "<!DOCTYPE html><html><head><meta http-equiv=\"refresh\" content=\"2\">" +
                "<meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">" +
                "<title>PicoEnvMonKt</title><style>" +
                "body{font-family:monospace;background:#0e1418;color:#f0f0f0;margin:2em}" +
                "h1{color:#26a69a}td{padding:.2em 1em .2em 0}" +
                ".s{color:#a0b4bc}</style></head><body><h1>PicoEnvMonKt</h1><table>"
        )
    private val pageTail = formatter.ascii("</body></html>")

    // Dynamic-middle framing, also cached as bytes: the middle is written
    // straight into pageBuf with the append helpers below, so the serve path
    // allocates nothing at all — every dynamic-string intern here was re-paid
    // per request forever, and that churn is what pinned the board's
    // gc_alloc_threshold at 64 (docs/mem-session-2026-08.md, C2).
    private val rowOpen = formatter.ascii("<tr><td class=\"s\">")
    private val rowMid = formatter.ascii("</td><td>")
    private val rowClose = formatter.ascii("</td></tr>")
    private val labelTemp = formatter.ascii("Temperature")
    private val labelHum = formatter.ascii("Humidity")
    private val labelPres = formatter.ascii("Pressure")
    private val labelAir = formatter.ascii("Air quality")
    private val labelLight = formatter.ascii("Light")
    private val labelOutdoor = formatter.ascii("Outdoor ($CITY)")
    private val dashes = formatter.ascii("--")
    private val unavailable = formatter.ascii("unavailable")
    private val unitC = formatter.ascii("C")
    private val unitF = formatter.ascii("F")
    private val unitPct = formatter.ascii(" %")
    private val unitHpa = formatter.ascii(" hPa")
    private val unitLx = formatter.ascii(" lx")
    private val unitIaq = formatter.ascii(" IAQ")
    private val footOpen = formatter.ascii("</table><p class=\"s\">")
    private val footClose = formatter.ascii("</p>")
    private val timeUnsynced = formatter.ascii("time not synced - ")
    private val utcSep = formatter.ascii(" UTC - ")
    private val ipPrefix = formatter.ascii("IP ")
    private val upPrefix = formatter.ascii(" - up ")
    private val indexHtml = formatter.ascii("index.html ")

    // HTTP/1.0 + Connection: close means body length = EOF — no Content-Length,
    // so the response heads are constants too. Per-request garbage matters: the
    // GC threshold counts ALLOCATIONS, and a server allocating few-but-large
    // objects outruns it byte-wise long before it fires (found as an OOM at a
    // 360 KB heap cap: table-growth steps need contiguous KB the accumulated
    // garbage had fragmented away).
    private val head200 =
        formatter.ascii("HTTP/1.0 200 OK\r\nContent-Type: text/html\r\nConnection: close\r\n\r\n")
    private val head404 =
        formatter.ascii(
            "HTTP/1.0 404 Not Found\r\nContent-Type: text/plain\r\nConnection: close\r\n\r\n" +
                "not found\n"
        )

    private val reqBuf = ByteArray(REQUEST_BUF_BYTES)

    /** Persistent page-assembly buffer — only the small dynamic middle allocates per request. */
    private val pageBuf = ByteArray(PAGE_BUF_BYTES)

    private var server: ServerSocket? = null

    /** Bind + listen if not already open. Returns false on bind failure (caller backs off). */
    fun ensureOpen(): Boolean {
        if (server != null) {
            return true
        }
        try {
            val s = ServerSocket(HTTP_PORT)
            s.setSoTimeout(ACCEPT_TIMEOUT_MS)
            server = s
            Log.i(TAG, "http: serving on port $HTTP_PORT")
            return true
        } catch (e: IOException) {
            Log.i(TAG, "http: bind failed: ${e.message}")
            server = null
            return false
        }
    }

    /**
     * Accept and serve at most one request, blocking up to the 1 s accept timeout. Total against
     * per-connection failures; only the accept timeout is silent (it is the normal idle path).
     */
    fun serveOnce() {
        val s = server ?: return
        var client: Socket? = null
        try {
            val c = s.accept()
            client = c
            c.setTimeout(CLIENT_TIMEOUT_MS)
            val lineLen = readRequest(c)
            if (isDashboardGet(lineLen)) {
                val pageLen = buildPage()
                sendAll(c, head200, head200.size)
                sendAll(c, pageBuf, pageLen)
            } else {
                // The only per-request allocation on this path is the log line.
                Log.i(TAG, "http: 404 for ${if (lineLen > 0) requestLine(lineLen) else "(bad)"}")
                sendAll(c, head404, head404.size)
            }
        } catch (e: SocketTimeoutException) {
            // No client this tick, or a client stalled mid-request — both routine.
        } catch (e: IOException) {
            Log.i(TAG, "http: connection error: ${e.message}")
        } catch (e: RuntimeException) {
            Log.i(TAG, "http: unexpected: $e")
        } finally {
            client?.close()
        }
    }

    fun close() {
        server?.close()
        server = null
    }

    /**
     * Read until the blank line ends the headers. Returns the request-line length in [reqBuf] (0 on
     * garbage) — the line stays as bytes; no String is built on the happy path.
     */
    private fun readRequest(client: Socket): Int {
        var total = 0
        while (total < reqBuf.size) {
            val n = client.recv(reqBuf, total, reqBuf.size - total)
            if (n < 0) {
                break // orderly EOF before a full request
            }
            total += n
            if (indexOfHeaderEnd(reqBuf, total) >= 0) {
                break
            }
        }
        // First line up to CR or LF.
        var end = 0
        while (
            end < total && reqBuf[end].toInt() != '\r'.code && reqBuf[end].toInt() != '\n'.code
        ) {
            end++
        }
        return end
    }

    /**
     * The request line as a String, for the 404 log only. `String(bytes, off, len)` in Kotlin
     * inlines to the `Charset` constructor pico-jvm lacks; appending chars is served.
     */
    private fun requestLine(len: Int): String {
        val sb = StringBuilder()
        for (i in 0 until len) {
            sb.append((reqBuf[i].toInt() and 0xFF).toChar())
        }
        return sb.toString()
    }

    /** Byte-level match for "GET / " / "GET /index.html " — allocation-free. */
    private fun isDashboardGet(lineLen: Int): Boolean {
        if (
            lineLen < 6 ||
                reqBuf[0].toInt() != 'G'.code ||
                reqBuf[1].toInt() != 'E'.code ||
                reqBuf[2].toInt() != 'T'.code ||
                reqBuf[3].toInt() != ' '.code ||
                reqBuf[4].toInt() != '/'.code
        ) {
            return false
        }
        if (reqBuf[5].toInt() == ' '.code) {
            return true
        }
        if (lineLen < 5 + indexHtml.size) {
            return false
        }
        for (i in 0 until indexHtml.size) {
            if (reqBuf[5 + i] != indexHtml[i]) {
                return false
            }
        }
        return true
    }

    private fun indexOfHeaderEnd(buf: ByteArray, len: Int): Int {
        val cr = '\r'.code
        val lf = '\n'.code
        for (i in 3 until len) {
            if (
                buf[i].toInt() == lf &&
                    buf[i - 1].toInt() == cr &&
                    buf[i - 2].toInt() == lf &&
                    buf[i - 3].toInt() == cr
            ) {
                return i
            }
        }
        return -1
    }

    /**
     * `Socket.send` writes at most one 256-byte native chunk per call and returns the count (the
     * documented NET-9 staging-buffer limit) — anything page-sized must loop.
     */
    private fun sendAll(client: Socket, buf: ByteArray, len: Int) {
        var off = 0
        while (off < len) {
            val n = client.send(buf, off, len - off)
            if (n <= 0) {
                throw IOException("send stalled at $off/$len")
            }
            off += n
        }
    }

    /** Assemble the page into [pageBuf]; returns its length. Allocation-free. */
    private fun buildPage(): Int {
        val latest = latestReadings
        val f = formatter
        var len = 0
        len = appendClamped(pageBuf, len, pageHead)

        len = rowStart(len, labelTemp)
        if (latest.isValid(IDX_TEMPERATURE)) {
            len = appendCenti(pageBuf, len, f.tempCenti(latest.get(IDX_TEMPERATURE)))
            len = appendClamped(pageBuf, len, if (f.isFahrenheit) unitF else unitC)
        } else {
            len = appendClamped(pageBuf, len, dashes)
        }
        len = appendClamped(pageBuf, len, rowClose)

        len = rowStart(len, labelHum)
        if (latest.isValid(IDX_HUMIDITY)) {
            len = appendCenti(pageBuf, len, f.centi(latest.get(IDX_HUMIDITY)))
            len = appendClamped(pageBuf, len, unitPct)
        } else {
            len = appendClamped(pageBuf, len, dashes)
        }
        len = appendClamped(pageBuf, len, rowClose)

        len = rowStart(len, labelPres)
        if (latest.isValid(IDX_PRESSURE)) {
            len = appendCenti(pageBuf, len, f.centi(latest.get(IDX_PRESSURE)))
            len = appendClamped(pageBuf, len, unitHpa)
        } else {
            len = appendClamped(pageBuf, len, dashes)
        }
        len = appendClamped(pageBuf, len, rowClose)

        len = rowStart(len, labelAir)
        val gas = if (latest.isValid(IDX_GAS)) latest.get(IDX_GAS) else 0f
        if (gas > 0f) {
            len = appendInt(pageBuf, len, f.iaqFromGas(gas))
            len = appendClamped(pageBuf, len, unitIaq)
        } else {
            len = appendClamped(pageBuf, len, dashes)
        }
        len = appendClamped(pageBuf, len, rowClose)

        len = rowStart(len, labelLight)
        if (latest.isValid(IDX_LIGHT)) {
            len = appendInt(pageBuf, len, latest.get(IDX_LIGHT).toInt())
            len = appendClamped(pageBuf, len, unitLx)
        } else {
            len = appendClamped(pageBuf, len, dashes)
        }
        len = appendClamped(pageBuf, len, rowClose)

        len = rowStart(len, labelOutdoor)
        len = appendClamped(pageBuf, len, net.weatherBytes ?: unavailable)
        len = appendClamped(pageBuf, len, rowClose)

        len = appendFooter(len)
        len = appendClamped(pageBuf, len, pageTail)
        return len
    }

    /** "HH:MM:SS UTC - IP a.b.c.d - up 3h 12m 45s" — TimeFormat.hms's math, byte-path. */
    private fun appendFooter(start: Int): Int {
        var off = appendClamped(pageBuf, start, footOpen)
        if (net.isTimeSynced) {
            val adjusted = System.currentTimeMillis() + UTC_OFFSET_MINUTES * 60_000L
            var daySec = (adjusted / 1000L) % 86_400L
            if (daySec < 0) {
                daySec += 86_400L
            }
            off = append2(pageBuf, off, (daySec / 3600).toInt())
            off = appendByte(pageBuf, off, ':'.code.toByte())
            off = append2(pageBuf, off, ((daySec % 3600) / 60).toInt())
            off = appendByte(pageBuf, off, ':'.code.toByte())
            off = append2(pageBuf, off, (daySec % 60).toInt())
            off = appendClamped(pageBuf, off, utcSep)
        } else {
            off = appendClamped(pageBuf, off, timeUnsynced)
        }
        off = appendClamped(pageBuf, off, ipPrefix)
        val ip = net.ipBytes
        if (ip != null) {
            off = appendClamped(pageBuf, off, ip)
        }
        off = appendClamped(pageBuf, off, upPrefix)
        val s = SystemClock.elapsedRealtimeNanos() / 1_000_000_000L
        off = appendInt(pageBuf, off, (s / 3600).toInt())
        off = appendByte(pageBuf, off, 'h'.code.toByte())
        off = appendByte(pageBuf, off, ' '.code.toByte())
        off = appendInt(pageBuf, off, ((s % 3600) / 60).toInt())
        off = appendByte(pageBuf, off, 'm'.code.toByte())
        off = appendByte(pageBuf, off, ' '.code.toByte())
        off = appendInt(pageBuf, off, (s % 60).toInt())
        off = appendByte(pageBuf, off, 's'.code.toByte())
        return appendClamped(pageBuf, off, footClose)
    }

    private fun rowStart(start: Int, label: ByteArray): Int {
        var off = appendClamped(pageBuf, start, rowOpen)
        off = appendClamped(pageBuf, off, label)
        return appendClamped(pageBuf, off, rowMid)
    }

    private fun appendClamped(dst: ByteArray, off: Int, src: ByteArray): Int {
        val n = minOf(src.size, dst.size - off)
        System.arraycopy(src, 0, dst, off, n)
        return off + n
    }

    private fun appendByte(dst: ByteArray, off: Int, b: Byte): Int {
        if (off < dst.size) {
            dst[off] = b
            return off + 1
        }
        return off
    }

    /** Decimal int → ASCII digits, clamped like [appendClamped]. */
    private fun appendInt(dst: ByteArray, start: Int, v: Int): Int {
        var off = start
        if (v < 0) {
            off = appendByte(dst, off, '-'.code.toByte())
        }
        val abs = if (v < 0) -v.toLong() else v.toLong()
        var div = 1L
        while (abs / div >= 10) {
            div *= 10
        }
        while (div > 0) {
            off = appendByte(dst, off, ('0'.code + (abs / div % 10).toInt()).toByte())
            div /= 10
        }
        return off
    }

    /** 1234 → "12.34" — the byte-path twin of Formatter's two-decimal formatting. */
    private fun appendCenti(dst: ByteArray, start: Int, centi: Int): Int {
        var off = start
        var c = centi
        if (c < 0) {
            off = appendByte(dst, off, '-'.code.toByte())
            c = -c
        }
        off = appendInt(dst, off, c / 100)
        off = appendByte(dst, off, '.'.code.toByte())
        return append2(dst, off, c % 100)
    }

    /** Two-digit zero-padded. */
    private fun append2(dst: ByteArray, start: Int, v: Int): Int {
        val off = appendByte(dst, start, ('0'.code + (v / 10) % 10).toByte())
        return appendByte(dst, off, ('0'.code + v % 10).toByte())
    }
}
