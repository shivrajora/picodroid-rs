#!/usr/bin/env python3
"""The test host's TLS listeners for the `net` rows of hil-tests.conf (https_get,
askclaude, weather). Four HTTPS ports, all TLS 1.3, serving the test-only certificates
in scripts/tls-test/ (regen.sh; a build trusts the CA through
PICODROID_TLS_EXTRA_CA=scripts/tls-test/test-ca.der):

    8443  a valid leaf for localhost / picodroid-test / 127.0.0.1 (+ --extra-ip)
          GET  /            -> 200, a short text body
          POST /v1/messages -> 200, a canned Claude Messages reply (any key accepted)
          GET /v1/forecast  -> 200, a canned open-meteo forecast (examples/weather)
    8444  the same names, but the leaf expired in 2020     -> must be refused
    8445  a self-signed leaf under no CA the build trusts  -> must be refused
    8446  a valid leaf for other.example only              -> must be refused

Started and stopped by scripts/net-lib.sh next to the echo and HTTP servers;
run it by hand for a developer sim session:

    ./scripts/tls-listener.py            # 0.0.0.0:8443-8446
    ./scripts/sim.sh --app https_get --board pico_display2_w
"""
import argparse
import json
import os
import ssl
import subprocess
import sys
import tempfile
import threading
import time
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

HERE = os.path.dirname(os.path.abspath(__file__))
TEST_DIR = os.path.join(HERE, "tls-test")
CA_CERT = os.path.join(TEST_DIR, "test-ca.pem")
CA_KEY = os.path.join(TEST_DIR, "test-ca-key.pem")

CANNED_REPLY = {
    "id": "msg_test_0001",
    "type": "message",
    "role": "assistant",
    "model": "claude-opus-5",
    "content": [{"type": "text", "text": "Hello from the picodroid test listener."}],
    "stop_reason": "end_turn",
    "stop_sequence": None,
    "usage": {"input_tokens": 12, "output_tokens": 9},
}


def canned_forecast():
    """An open-meteo `/v1/forecast` reply for examples/weather's nightly row:
    the shape the app requests (`timeformat=unixtime`, `timezone=auto`, the
    current conditions, 12 hours, 7 days), a partly cloudy 18.3 C afternoon
    in San Mateo, its times counted from the listener's own clock so the
    strip and the list read as today's."""
    now = int(time.time()) // 3600 * 3600
    day = now // 86400 * 86400
    hourly_temp = [18.3, 18.1, 17.6, 16.8, 15.9, 15.0, 14.2, 13.6, 13.1, 12.7, 12.3, 12.0]
    hourly_code = [2, 2, 1, 1, 0, 0, 0, 0, 3, 3, 61, 61]
    daily_code = [2, 3, 61, 80, 1, 0, 95]
    daily_max = [21.4, 19.8, 17.2, 16.5, 20.1, 23.7, 19.0]
    daily_min = [12.1, 12.8, 11.4, 10.9, 11.7, 13.2, 12.5]
    return {
        "latitude": 37.56, "longitude": -122.32, "generationtime_ms": 0.2,
        "utc_offset_seconds": -25200, "timezone": "America/Los_Angeles",
        "timezone_abbreviation": "PDT", "elevation": 5.0,
        "current_units": {"time": "unixtime", "interval": "seconds",
                          "temperature_2m": "°C", "relative_humidity_2m": "%",
                          "apparent_temperature": "°C", "is_day": "",
                          "weather_code": "wmo code", "surface_pressure": "hPa",
                          "wind_speed_10m": "km/h"},
        "current": {"time": now, "interval": 900, "temperature_2m": 18.3,
                    "relative_humidity_2m": 62, "apparent_temperature": 17.1,
                    "is_day": 1, "weather_code": 2, "surface_pressure": 1013.2,
                    "wind_speed_10m": 12.4},
        "hourly_units": {"time": "unixtime", "temperature_2m": "°C",
                         "weather_code": "wmo code", "is_day": ""},
        "hourly": {"time": [now + 3600 * i for i in range(12)],
                   "temperature_2m": hourly_temp, "weather_code": hourly_code,
                   "is_day": [1, 1, 1, 1, 1, 0, 0, 0, 0, 0, 0, 0]},
        "daily_units": {"time": "unixtime", "weather_code": "wmo code",
                        "temperature_2m_max": "°C", "temperature_2m_min": "°C",
                        "sunrise": "unixtime", "sunset": "unixtime"},
        "daily": {"time": [day + 86400 * i for i in range(7)],
                  "weather_code": daily_code, "temperature_2m_max": daily_max,
                  "temperature_2m_min": daily_min,
                  "sunrise": [day + 86400 * i + 25200 + 6 * 3600 + 52 * 60 for i in range(7)],
                  "sunset": [day + 86400 * i + 25200 + 19 * 3600 + 8 * 60 for i in range(7)]},
    }


class Handler(BaseHTTPRequestHandler):
    protocol_version = "HTTP/1.1"
    server_version = "picodroid-tls-test/1"

    def _reply(self, status, body, content_type="text/plain"):
        data = body if isinstance(body, bytes) else body.encode()
        self.send_response(status)
        self.send_header("Content-Type", content_type)
        self.send_header("Content-Length", str(len(data)))
        self.send_header("Connection", "close")
        self.end_headers()
        self.wfile.write(data)

    def do_GET(self):
        if self.path == "/":
            self._reply(200, "picodroid tls ok\n")
        elif self.path.startswith("/v1/forecast"):
            self._reply(200, json.dumps(canned_forecast()), "application/json")
        else:
            self._reply(404, "not found\n")

    def do_POST(self):
        length = int(self.headers.get("Content-Length") or 0)
        body = self.rfile.read(length) if length else b""
        if self.path == "/v1/messages":
            reply = dict(CANNED_REPLY)
            try:
                prompt = json.loads(body.decode())["messages"][0]["content"]
                if isinstance(prompt, str):
                    reply["usage"] = {"input_tokens": max(1, len(prompt) // 4), "output_tokens": 9}
            except (ValueError, KeyError, IndexError, TypeError):
                pass
            self._reply(200, json.dumps(reply), "application/json")
        else:
            self._reply(404, "not found\n")

    def log_message(self, fmt, *args):
        sys.stderr.write("%s:%d - %s\n" % (self.client_address[0], self.server.server_port, fmt % args))


def run(cmd):
    subprocess.run(cmd, check=True, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)


def committed(name):
    """A leaf scripts/tls-test/regen.sh committed: (cert, key) PEM paths."""
    return os.path.join(TEST_DIR, name + ".pem"), os.path.join(TEST_DIR, name + "-key.pem")


def mint_good(workdir, san):
    """The valid leaf with an extra iPAddress (the bench host's LAN IP): minted
    under the test CA at startup, since the committed one names only loopback."""
    key = os.path.join(workdir, "good-key.pem")
    cert = os.path.join(workdir, "good.pem")
    csr = os.path.join(workdir, "good.csr")
    ext = os.path.join(workdir, "good.ext")
    run(["openssl", "ecparam", "-name", "prime256v1", "-genkey", "-noout", "-out", key])
    with open(ext, "w") as f:
        f.write("basicConstraints=CA:FALSE\nkeyUsage=critical,digitalSignature\n"
                "extendedKeyUsage=serverAuth\nsubjectAltName=%s\n" % san)
    run(["openssl", "req", "-new", "-key", key, "-sha256", "-subj", "/O=picodroid test/CN=localhost",
         "-out", csr])
    # The serial file goes to the work dir, not next to the committed CA.
    run(["openssl", "x509", "-req", "-in", csr, "-CA", CA_CERT, "-CAkey", CA_KEY,
         "-CAserial", os.path.join(workdir, "serial"), "-CAcreateserial",
         "-sha256", "-days", "3650", "-extfile", ext, "-out", cert])
    return cert, key


def serve(bind, port, cert, key):
    ctx = ssl.SSLContext(ssl.PROTOCOL_TLS_SERVER)
    ctx.minimum_version = ssl.TLSVersion.TLSv1_3
    ctx.load_cert_chain(cert, key)
    httpd = ThreadingHTTPServer((bind, port), Handler)
    httpd.socket = ctx.wrap_socket(httpd.socket, server_side=True)
    t = threading.Thread(target=httpd.serve_forever, daemon=True)
    t.start()
    return httpd


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--bind", default="0.0.0.0")
    ap.add_argument("--port", type=int, default=8443, help="the valid port; the refusals follow it")
    ap.add_argument("--extra-ip", default="", help="an extra iPAddress SAN (the bench host's LAN IP)")
    args = ap.parse_args()

    san = "DNS:localhost,DNS:picodroid-test,IP:127.0.0.1"
    if args.extra_ip:
        san += ",IP:" + args.extra_ip
        good = mint_good(tempfile.mkdtemp(prefix="picodroid-tls-"), san)
    else:
        good = committed("localhost")
    ports = [
        (args.port, good),
        (args.port + 1, committed("expired")),
        (args.port + 2, committed("untrusted")),
        (args.port + 3, committed("wrongname")),
    ]
    servers = [serve(args.bind, port, cert, key) for port, (cert, key) in ports]
    sys.stderr.write("tls-listener: https://%s:%d/ good, %d expired, %d untrusted, %d wrong-name (SAN %s)\n"
                     % (args.bind, ports[0][0], ports[1][0], ports[2][0], ports[3][0], san))
    sys.stderr.flush()
    try:
        threading.Event().wait()
    except KeyboardInterrupt:
        pass
    finally:
        for s in servers:
            s.shutdown()


if __name__ == "__main__":
    main()
