#!/usr/bin/env python3
"""The test host's TLS listeners for the `net` rows of hil-tests.conf (https_get,
askclaude). Four HTTPS ports, all TLS 1.3, serving the test-only certificates
in scripts/tls-test/ (regen.sh; a build trusts the CA through
PICODROID_TLS_EXTRA_CA=scripts/tls-test/test-ca.der):

    8443  a valid leaf for localhost / picodroid-test / 127.0.0.1 (+ --extra-ip)
          GET  /            -> 200, a short text body
          POST /v1/messages -> 200, a canned Claude Messages reply (any key accepted)
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
