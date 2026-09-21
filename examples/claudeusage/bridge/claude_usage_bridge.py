#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-only
"""LAN bridge for the picodroid `claudeusage` app.

The device has no TLS, so this script runs on the PC that uses Claude Code and
serves the numbers over plain HTTP on the LAN:

    GET /u   ->  one compact JSON object, at most MAX_BODY bytes

It reads two things:
  * the subscription limits (5-hour session, weekly, per-model weekly) from
    Anthropic's OAuth usage endpoint, with the token Claude Code keeps in
    ~/.claude/.credentials.json. The token is re-read on every poll and never
    leaves this process: the device only ever sees derived numbers.
  * token counts from the local transcripts under ~/.claude/projects/.

The usage endpoint is undocumented and rate limits hard, so it is polled no
faster than every UPSTREAM_PERIOD_S seconds and everything is parsed defensively.

    ./claude_usage_bridge.py                 # real data
    ./claude_usage_bridge.py --once          # print one payload and exit
    ./claude_usage_bridge.py --demo          # synthetic data, no account needed
    ./claude_usage_bridge.py --demo --fail auth
    curl 'localhost:8787/demo?fail=hang'     # (demo only) switch failure mode live
    curl 'localhost:8787/demo?reset=30'      # (demo only) session resets 30 s from now

Standard library only.
"""

import argparse
import datetime as dt
import glob
import json
import math
import os
import subprocess
import sys
import threading
import time
import urllib.error
import urllib.request
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

USAGE_URL = "https://api.anthropic.com/api/oauth/usage"
CREDENTIALS = os.path.expanduser("~/.claude/.credentials.json")
TRANSCRIPTS = os.path.expanduser("~/.claude/projects")
UPSTREAM_PERIOD_S = 180
TRANSCRIPT_PERIOD_S = 60
MAX_BODY = 700  # the device reads into a fixed 1 KiB buffer
HISTORY_DAYS = 7
RATE_WINDOW_S = 20 * 60
FAIL_MODES = ("none", "auth", "rate", "creds", "garbage", "http500", "hang", "nodata")

# USD per million tokens (input, output, cache write, cache read), matched on a
# substring of the model id. An ESTIMATE of API-equivalent cost: subscription
# usage is not billed per token. Unknown models count tokens but no cost.
PRICES = (
    ("opus", (15.0, 75.0, 18.75, 1.50)),
    ("sonnet", (3.0, 15.0, 3.75, 0.30)),
    ("haiku", (1.0, 5.0, 1.25, 0.10)),
)


def log(msg):
    print(time.strftime("%H:%M:%S"), msg, flush=True)


def family(model):
    """'claude-opus-4-5-2025...' -> 'Opus'. Anything unrecognised keeps a short id."""
    m = (model or "").lower()
    for name in ("opus", "sonnet", "haiku", "fable", "mythos"):
        if name in m:
            return name.capitalize()
    m = m.replace("claude-", "")
    return (m[:8] or "other").capitalize()


def tz_offset_minutes():
    return int(dt.datetime.now().astimezone().utcoffset().total_seconds() // 60)


def parse_reset(value):
    """ISO-8601 (or epoch) reset time -> epoch seconds, 0 when absent."""
    if value is None:
        return 0
    if isinstance(value, (int, float)):
        return int(value)
    try:
        return int(dt.datetime.fromisoformat(str(value).replace("Z", "+00:00")).timestamp())
    except ValueError:
        return 0


def pct(value):
    try:
        return max(0, min(100, int(round(float(value)))))
    except (TypeError, ValueError):
        return -1


# --------------------------------------------------------------------------
# Limits: the OAuth usage endpoint
# --------------------------------------------------------------------------


class Limits:
    def __init__(self):
        self.lock = threading.Lock()
        self.session = None  # (pct, reset_epoch)
        self.weekly = None
        self.models = []  # [(label, pct, reset_epoch)]
        self.plan = ""
        self.err = "init"
        self.fetched_at = 0.0
        self.samples = []  # (time, session pct as float) for the burn rate
        self.user_agent = "claude-code/" + self._cli_version()

    @staticmethod
    def _cli_version():
        try:
            out = subprocess.run(
                ["claude", "--version"], capture_output=True, text=True, timeout=10
            ).stdout.split()
            return out[0] if out else "2.0.0"
        except (OSError, subprocess.SubprocessError):
            return "2.0.0"

    def poll(self):
        try:
            with open(CREDENTIALS, encoding="utf-8") as f:
                oauth = json.load(f).get("claudeAiOauth") or {}
            token = oauth.get("accessToken")
            if not token:
                raise KeyError("accessToken")
        except (OSError, ValueError, KeyError):
            self._fail("creds")
            return
        if oauth.get("expiresAt") and oauth["expiresAt"] / 1000 < time.time():
            # Claude Code refreshes the file the next time it runs; the bridge
            # never uses the refresh token itself.
            self._fail("auth")
            return
        req = urllib.request.Request(
            USAGE_URL,
            headers={
                "Authorization": "Bearer " + token,
                "anthropic-beta": "oauth-2025-04-20",
                "User-Agent": self.user_agent,
                "Accept": "application/json",
            },
        )
        try:
            with urllib.request.urlopen(req, timeout=15) as resp:
                body = json.load(resp)
        except urllib.error.HTTPError as e:
            self._fail({401: "auth", 403: "auth", 429: "rate"}.get(e.code, "http%d" % e.code))
            return
        except (urllib.error.URLError, OSError, ValueError):
            self._fail("net")
            return
        self._accept(body, str(oauth.get("subscriptionType") or ""))

    def _fail(self, err):
        with self.lock:
            if err != self.err:
                log("limits: upstream failed: " + err)
            self.err = err

    def _accept(self, body, plan):
        def window(key):
            node = body.get(key)
            if not isinstance(node, dict) or node.get("utilization") is None:
                return None
            return (pct(node["utilization"]), parse_reset(node.get("resets_at")), node)

        session, weekly = window("five_hour"), window("seven_day")
        models = []
        for key in sorted(body):
            # seven_day_opus, seven_day_sonnet, ... whatever the account reports.
            if key.startswith("seven_day_") and key != "seven_day_oauth_apps":
                w = window(key)
                if w:
                    models.append((key[len("seven_day_"):].capitalize()[:8], w[0], w[1]))
        now = time.time()
        with self.lock:
            if self.err:
                log("limits: ok (session %s%%)" % (session[0] if session else "?"))
            self.session = session[:2] if session else None
            self.weekly = weekly[:2] if weekly else None
            self.models = models[:3]
            self.plan = plan[:8]
            self.err = "" if (session or weekly) else "nodata"
            self.fetched_at = now
            if session:
                value = float(session[2]["utilization"])
                if self.samples and value < self.samples[-1][1] - 0.5:
                    self.samples = []  # the window rolled over
                self.samples.append((now, value))
                self.samples = [s for s in self.samples if now - s[0] <= RATE_WINDOW_S]

    def burn(self):
        """(percent per hour, minutes until 100 % or -1). Caller holds the lock."""
        if len(self.samples) < 2 or not self.session:
            return 0, -1
        (t0, p0), (t1, p1) = self.samples[0], self.samples[-1]
        if t1 - t0 < 60:
            return 0, -1
        rate = (p1 - p0) / (t1 - t0) * 3600.0
        if rate < 0.5:
            return 0, -1
        return int(round(rate)), int((100.0 - p1) / rate * 60.0)


# --------------------------------------------------------------------------
# History: local transcripts
# --------------------------------------------------------------------------


class History:
    """Incremental scan of ~/.claude/projects/**/*.jsonl.

    Claude Code rewrites an assistant message several times while it streams, so
    entries are keyed by message id and the last copy wins.
    """

    def __init__(self):
        self.lock = threading.Lock()
        self.offsets = {}  # path -> bytes consumed
        self.messages = {}  # message id -> (day ordinal, family, tokens, usd)

    def scan(self):
        horizon = time.time() - (HISTORY_DAYS + 1) * 86400
        for path in glob.glob(os.path.join(TRANSCRIPTS, "**", "*.jsonl"), recursive=True):
            try:
                st = os.stat(path)
                if st.st_mtime < horizon:
                    continue
                start = self.offsets.get(path, 0)
                if st.st_size < start:
                    start = 0
                if st.st_size == start:
                    continue
                with open(path, "rb") as f:
                    f.seek(start)
                    chunk = f.read()
            except OSError:
                continue
            end = chunk.rfind(b"\n") + 1  # leave a half-written last line for next time
            self.offsets[path] = start + end
            for line in chunk[:end].splitlines():
                self._line(line)
        first_day = dt.date.today().toordinal() - HISTORY_DAYS
        with self.lock:
            for mid in [m for m, v in self.messages.items() if v[0] < first_day]:
                del self.messages[mid]

    def _line(self, line):
        if b'"usage"' not in line:
            return
        try:
            entry = json.loads(line)
            msg = entry["message"]
            usage = msg["usage"]
            mid = msg.get("id") or entry.get("uuid")
            when = dt.datetime.fromisoformat(entry["timestamp"].replace("Z", "+00:00"))
        except (ValueError, KeyError, TypeError, AttributeError):
            return
        if not mid or not isinstance(usage, dict):
            return
        model = msg.get("model") or ""
        if model.startswith("<"):  # "<synthetic>"
            return
        tin = int(usage.get("input_tokens") or 0)
        tout = int(usage.get("output_tokens") or 0)
        twrite = int(usage.get("cache_creation_input_tokens") or 0)
        tread = int(usage.get("cache_read_input_tokens") or 0)
        usd = -1.0  # no price known for this model
        for needle, (pin, pout, pwrite, pread) in PRICES:
            if needle in model.lower():
                usd = (tin * pin + tout * pout + twrite * pwrite + tread * pread) / 1e6
                break
        day = when.astimezone().date().toordinal()
        # Cache reads are excluded from the token figure: they dwarf everything
        # else and say little about how much work was done.
        with self.lock:
            self.messages[mid] = (day, family(model), tin + tout + twrite, usd)

    def summary(self):
        today = dt.date.today().toordinal()
        days = [0] * HISTORY_DAYS
        tok = msgs = 0
        usd = 0.0
        unpriced = False
        mix = {}
        with self.lock:
            for day, fam, tokens, cost in self.messages.values():
                idx = HISTORY_DAYS - 1 - (today - day)
                if 0 <= idx < HISTORY_DAYS:
                    days[idx] += tokens
                    mix[fam] = mix.get(fam, 0) + tokens
                if day == today:
                    tok += tokens
                    msgs += 1
                    if cost < 0:
                        unpriced = True
                    else:
                        usd += cost
        total = sum(mix.values()) or 1
        top = sorted(mix.items(), key=lambda kv: -kv[1])[:3]
        letters = "".join(
            "MTWTFSS"[dt.date.fromordinal(today - (HISTORY_DAYS - 1 - i)).weekday()]
            for i in range(HISTORY_DAYS)
        )
        return {
            # usd is -1 when any of today's tokens came from a model with no entry in PRICES:
            # a partial sum would read as a real, and wrong, figure.
            "td": {"tok": tok // 1000, "usd": -1 if unpriced else int(round(usd * 100)), "msg": msgs},
            "d7": [d // 1000 for d in days],
            "dl": letters,
            "mix": [[name, int(round(100.0 * t / total))] for name, t in top if t],
        }


# --------------------------------------------------------------------------
# Payload
# --------------------------------------------------------------------------


def build_payload(limits, history):
    now = time.time()
    out = {"v": 1, "t": int(now), "tz": tz_offset_minutes()}
    with limits.lock:
        out["ok"] = 0 if limits.err else 1
        if limits.err:
            out["err"] = limits.err
        out["age"] = int(now - limits.fetched_at) if limits.fetched_at else -1
        if limits.plan:
            out["plan"] = limits.plan
        if limits.session:
            out["s"] = {"p": limits.session[0], "r": limits.session[1]}
        if limits.weekly:
            out["w"] = {"p": limits.weekly[0], "r": limits.weekly[1]}
        if limits.models:
            out["wm"] = [list(m) for m in limits.models]
        out["rate"], out["eta"] = limits.burn()
    out.update(history.summary())
    return encode(out)


def encode(out):
    body = json.dumps(out, separators=(",", ":")).encode()
    # Shed the optional parts rather than ever overrun the device's buffer.
    for key in ("mix", "wm", "d7", "dl"):
        if len(body) <= MAX_BODY:
            break
        out.pop(key, None)
        body = json.dumps(out, separators=(",", ":")).encode()
    return body


def demo_payload(started, fail):
    """Synthetic data that moves fast enough to watch: a session fills in ~10 min."""
    now = time.time()
    out = {"v": 1, "t": int(now), "tz": tz_offset_minutes(), "plan": "max"}
    if fail in ("auth", "rate", "creds"):
        out.update(ok=0, err=fail, age=-1)
        return encode(out)
    if fail == "nodata":
        out.update(ok=0, err="nodata", age=-1)
        return encode(out)
    phase = ((now - started) % 600.0) / 600.0
    session = int(phase * 104)  # overshoots to pin at 100 for a while
    weekly = 38 + int(phase * 20)
    today = dt.date.today().toordinal()
    out.update(
        ok=1,
        age=int(now - started) % 60,
        s={"p": min(100, session), "r": int(now + (1.0 - phase) * 5 * 3600)},
        w={"p": weekly, "r": int(now + 3 * 86400 + 4 * 3600)},
        wm=[["Opus", min(100, weekly + 30), int(now + 3 * 86400)], ["Sonnet", 12, int(now + 86400)]],
        rate=int(8 + 40 * phase),
        eta=-1 if session >= 100 else int((100 - session) / (8 + 40 * phase) * 60),
        td={"tok": int(900 + 2400 * phase), "usd": int(450 + 1900 * phase), "msg": int(40 + 300 * phase)},
        d7=[1200, 3400, 800, 0, 5100, 2600, int(900 + 2400 * phase)],
        dl="".join("MTWTFSS"[dt.date.fromordinal(today - 6 + i).weekday()] for i in range(7)),
        mix=[["Opus", 61], ["Sonnet", 34], ["Haiku", 5]],
    )
    if State.demo_reset:
        out["s"]["r"] = State.demo_reset
    return encode(out)


# --------------------------------------------------------------------------
# HTTP
# --------------------------------------------------------------------------


class State:
    limits = None
    history = None
    demo = False
    fail = "none"
    started = time.time()
    demo_reset = 0  # demo only: a fixed session reset time, epoch seconds


class Handler(BaseHTTPRequestHandler):
    server_version = "claude-usage-bridge/1"
    protocol_version = "HTTP/1.0"

    def do_GET(self):
        path, _, query = self.path.partition("?")
        if path == "/demo" and State.demo:
            key, _, mode = query.partition("=")
            if key == "reset" and mode.isdigit():
                State.demo_reset = int(time.time()) + int(mode)
                log("demo: session resets in %s s" % mode)
            elif mode in FAIL_MODES:
                State.fail = mode
                log("demo: fail mode -> " + mode)
            self._send(200, ("fail=" + State.fail + "\n").encode(), "text/plain")
            return
        if path not in ("/u", "/u/"):
            self._send(404, b"not found\n", "text/plain")
            return
        if State.demo:
            if State.fail == "hang":
                time.sleep(30)  # longer than the device's read timeout
                return
            if State.fail == "http500":
                self._send(500, b"boom\n", "text/plain")
                return
            if State.fail == "garbage":
                self._send(200, b'{"v":1,"s":{"p":', "application/json")
                return
            self._send(200, demo_payload(State.started, State.fail), "application/json")
            return
        self._send(200, build_payload(State.limits, State.history), "application/json")

    def _send(self, code, body, ctype):
        self.send_response(code)
        self.send_header("Content-Type", ctype)
        self.send_header("Content-Length", str(len(body)))
        self.send_header("Connection", "close")
        self.end_headers()
        try:
            self.wfile.write(body)
        except OSError:
            pass

    def log_message(self, fmt, *args):
        if os.environ.get("BRIDGE_VERBOSE"):
            log("http: " + fmt % args)


def poller(limits, history):
    next_limits = next_history = 0.0
    while True:
        now = time.time()
        if now >= next_history:
            try:
                history.scan()
            except Exception as e:  # noqa: BLE001 - never let the poller die
                log("history: scan failed: %r" % (e,))
            next_history = now + TRANSCRIPT_PERIOD_S
        if now >= next_limits:
            try:
                limits.poll()
            except Exception as e:  # noqa: BLE001
                limits._fail("bridge")
                log("limits: poll crashed: %r" % (e,))
            next_limits = now + UPSTREAM_PERIOD_S
        time.sleep(1)


def main():
    ap = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    ap.add_argument("--host", default="0.0.0.0")
    ap.add_argument("--port", type=int, default=8787)
    ap.add_argument("--demo", action="store_true", help="serve synthetic data")
    ap.add_argument("--fail", choices=FAIL_MODES, default="none", help="demo failure mode")
    ap.add_argument("--once", action="store_true", help="print one payload and exit")
    args = ap.parse_args()

    State.demo, State.fail = args.demo, args.fail
    if args.once:
        if args.demo:
            body = demo_payload(State.started, args.fail)
        else:
            limits, history = Limits(), History()
            history.scan()
            limits.poll()
            body = build_payload(limits, history)
        print(body.decode())
        print("%d bytes (limit %d)" % (len(body), MAX_BODY), file=sys.stderr)
        return 0 if len(body) <= MAX_BODY else 1

    if not args.demo:
        State.limits, State.history = Limits(), History()
        threading.Thread(target=poller, args=(State.limits, State.history), daemon=True).start()
    server = ThreadingHTTPServer((args.host, args.port), Handler)
    server.daemon_threads = True
    log("serving %s data on http://%s:%d/u" % ("DEMO" if args.demo else "live", args.host, args.port))
    try:
        server.serve_forever()
    except KeyboardInterrupt:
        pass
    return 0


if __name__ == "__main__":
    sys.exit(main())
