#!/usr/bin/env python3
"""Send an HTML email report for a picodroid test run (HIL or sim).

Uses Gmail SMTP with an App Password. Credentials are read from
~/.config/picodroid/hil-email.conf:

    GMAIL_USER=you@gmail.com
    GMAIL_APP_PASSWORD=xxxx xxxx xxxx xxxx

Usage:
    python3 hil-email.py --results <file> --log-dir <dir> --run-id <id> --sha <sha>
    python3 hil-email.py --results <file> ... --suite sim
    python3 hil-email.py --results <file> ... --to someone@example.com
"""

import argparse
import os
import smtplib
import sys
from email.mime.multipart import MIMEMultipart
from email.mime.text import MIMEText
from pathlib import Path

DEFAULT_RECIPIENT = None  # set via --to flag or GMAIL_USER from config
CONF_PATH = Path.home() / ".config" / "picodroid" / "hil-email.conf"
SMTP_HOST = "smtp.gmail.com"
SMTP_PORT = 587
LOG_TAIL_LINES = 30


def load_credentials():
    """Read GMAIL_USER and GMAIL_APP_PASSWORD from config file."""
    if not CONF_PATH.exists():
        print(f"ERROR: credentials file not found: {CONF_PATH}", file=sys.stderr)
        print("Create it with:", file=sys.stderr)
        print(f"  mkdir -p {CONF_PATH.parent}", file=sys.stderr)
        print(f"  cat > {CONF_PATH} <<'EOF'", file=sys.stderr)
        print("GMAIL_USER=you@gmail.com", file=sys.stderr)
        print("GMAIL_APP_PASSWORD=xxxx xxxx xxxx xxxx", file=sys.stderr)
        print("EOF", file=sys.stderr)
        print(f"  chmod 600 {CONF_PATH}", file=sys.stderr)
        sys.exit(1)

    creds = {}
    for line in CONF_PATH.read_text().splitlines():
        line = line.strip()
        if not line or line.startswith("#"):
            continue
        key, _, value = line.partition("=")
        creds[key.strip()] = value.strip()

    user = creds.get("GMAIL_USER")
    password = creds.get("GMAIL_APP_PASSWORD")
    if not user or not password:
        print(f"ERROR: GMAIL_USER and GMAIL_APP_PASSWORD must be set in {CONF_PATH}", file=sys.stderr)
        sys.exit(1)
    return user, password


def parse_results(results_path):
    """Parse the results file into a list of (status, app) tuples."""
    entries = []
    for line in Path(results_path).read_text().splitlines():
        line = line.strip()
        if not line:
            continue
        status, _, app = line.partition(" ")
        entries.append((status.strip(), app.strip()))
    return entries


def load_history(results_path):
    """Load prior runs' results from sibling files, newest first.

    Results files are named `YYYY-MM-DD_HHhMMmSSs_<sha>.txt`, so lexical order
    is chronological order. Returns a list of (date, {app: status}) tuples for
    every run strictly older than the current one.
    """
    cur = Path(results_path).resolve()
    history = []
    for f in sorted(cur.parent.glob("*.txt"), reverse=True):
        if f.name >= cur.name:
            continue
        run_map = {app: status for status, app in parse_results(f)}
        history.append((f.name[:10], run_map))
    return history


def classify_failures(entries, history):
    """Split current failures into new-vs-known, and find recoveries.

    A failure is "known" if the most recent prior run that *included* the app
    also failed it; its value is (since_date, open_ended) where since_date
    starts the contiguous failing streak and open_ended means the streak
    covers all recorded history (so it may be older still). Runs that don't
    mention the app (partial `--app` invocations) are skipped rather than
    treated as evidence either way. "Recovered" lists apps that last failed
    and PASS now. Without history, every failure counts as new.

    The point: a permanently-red suite must not look the same as a fresh
    regression — that disguise hid a month of red nightlies in 2026-05/06.
    """
    cur_failing = sorted(app for status, app in entries if status in ("FAIL", "ERROR"))
    cur_map = {app: status for status, app in entries}

    def walk_streak(app):
        """Date of the oldest run in the app's contiguous failing streak."""
        since = None
        saw_all_history = True
        for date, run_map in history:
            if app not in run_map:
                continue
            if run_map[app] in ("FAIL", "ERROR"):
                since = date
            else:
                saw_all_history = False
                break
        return since, saw_all_history

    new_fails = []
    known_fails = {}  # app -> (since_date, open_ended)
    for app in cur_failing:
        since, open_ended = walk_streak(app)
        if since is None:
            new_fails.append(app)
        else:
            known_fails[app] = (since, open_ended)

    # Recovered: PASSes now, and the most recent prior run including the app
    # failed it.
    recovered = []
    for app, status in cur_map.items():
        if status != "PASS":
            continue
        for _, run_map in history:
            if app in run_map:
                if run_map[app] in ("FAIL", "ERROR"):
                    recovered.append(app)
                break
    recovered.sort()
    return new_fails, known_fails, recovered


def read_log_tail(log_dir, run_id, app, max_lines=LOG_TAIL_LINES):
    """Read the last N lines of an app's log (RTT or PDB)."""
    run_dir = Path(log_dir) / run_id

    # PDB test: app name is "helloworld:pdb-ping" → log is "helloworld.pdb-ping.log"
    if ":" in app:
        base_app, _, pdb_suffix = app.partition(":")
        log_file = run_dir / f"{base_app}.{pdb_suffix}.log"
        if log_file.exists():
            lines = log_file.read_text().splitlines()
            tail = lines[-max_lines:] if len(lines) > max_lines else lines
            return "\n".join(tail)

    # Standard layout: logs/<run_id>/<app>.log; fall back to flat naming for old runs.
    log_file = run_dir / f"{app}.log"
    if not log_file.exists():
        log_file = Path(log_dir) / f"{run_id}-{app}.log"
    if not log_file.exists():
        return "(no log file found)"
    lines = log_file.read_text().splitlines()
    tail = lines[-max_lines:] if len(lines) > max_lines else lines
    return "\n".join(tail)


def format_known_since(since, open_ended):
    """Human-readable streak start for a known failure."""
    if since is None:
        return "failing since unknown"
    return f"failing since {since}" + (" or earlier" if open_ended else "")


def build_triage_html(new_fails, known_fails, recovered):
    """Build the new-vs-known triage section shown above the results table."""
    parts = []
    if new_fails:
        items = "".join(f"<li><strong>{app}</strong></li>" for app in new_fails)
        parts.append(
            f'<div style="background:#ffebee;border-left:4px solid #c62828;'
            f'padding:10px 14px;margin-top:12px;">'
            f'<strong style="color:#c62828;">NEW failures (not in previous run)</strong>'
            f'<ul style="margin:6px 0 0;">{items}</ul></div>'
        )
    if known_fails:
        items = "".join(
            f"<li>{app} <span style=\"color:#9e9e9e;\">— "
            f"{format_known_since(since, open_ended)}</span></li>"
            for app, (since, open_ended) in sorted(known_fails.items())
        )
        parts.append(
            f'<div style="background:#fff3e0;border-left:4px solid #e65100;'
            f'padding:10px 14px;margin-top:12px;">'
            f'<strong style="color:#e65100;">Known failures</strong>'
            f'<ul style="margin:6px 0 0;">{items}</ul></div>'
        )
    if recovered:
        items = "".join(f"<li>{app}</li>" for app in recovered)
        parts.append(
            f'<div style="background:#e8f5e9;border-left:4px solid #2e7d32;'
            f'padding:10px 14px;margin-top:12px;">'
            f'<strong style="color:#2e7d32;">Recovered since previous run</strong>'
            f'<ul style="margin:6px 0 0;">{items}</ul></div>'
        )
    return "".join(parts)


def build_html(entries, log_dir, run_id, sha, suite="HIL", triage_html=""):
    """Build an HTML email body with colour-coded results."""
    colors = {
        "PASS": "#2e7d32",
        "FAIL": "#c62828",
        "ERROR": "#e65100",
        "SKIP": "#9e9e9e",
    }
    badge_bg = {
        "PASS": "#e8f5e9",
        "FAIL": "#ffebee",
        "ERROR": "#fff3e0",
        "SKIP": "#f5f5f5",
    }

    counts = {"PASS": 0, "FAIL": 0, "ERROR": 0, "SKIP": 0}
    for status, _ in entries:
        counts[status] = counts.get(status, 0) + 1

    total_run = counts["PASS"] + counts["FAIL"] + counts["ERROR"]
    all_passed = counts["FAIL"] == 0 and counts["ERROR"] == 0

    # Header banner.
    if all_passed:
        banner_bg = "#2e7d32"
        banner_text = f"ALL {total_run} TESTS PASSED"
    else:
        banner_bg = "#c62828"
        failed = counts["FAIL"] + counts["ERROR"]
        banner_text = f"{failed} TEST{'S' if failed != 1 else ''} FAILED"

    rows = []
    failure_details = []

    for status, app in entries:
        color = colors.get(status, "#000")
        bg = badge_bg.get(status, "#fff")
        rows.append(
            f'<tr>'
            f'<td style="padding:6px 12px;border-bottom:1px solid #eee;">{app}</td>'
            f'<td style="padding:6px 12px;border-bottom:1px solid #eee;">'
            f'<span style="background:{bg};color:{color};padding:2px 8px;'
            f'border-radius:3px;font-weight:bold;font-size:12px;">{status}</span></td>'
            f'</tr>'
        )

        if status in ("FAIL", "ERROR"):
            log_text = read_log_tail(log_dir, run_id, app)
            failure_details.append(
                f'<div style="margin:12px 0;">'
                f'<strong style="color:{color};">{app}</strong>'
                f'<pre style="background:#f5f5f5;padding:10px;border-radius:4px;'
                f'font-size:12px;overflow-x:auto;max-height:300px;">{log_text}</pre>'
                f'</div>'
            )

    html = f"""\
<html>
<body style="font-family:-apple-system,BlinkMacSystemFont,sans-serif;color:#333;max-width:600px;">
  <div style="background:{banner_bg};color:white;padding:16px 20px;border-radius:6px 6px 0 0;">
    <h2 style="margin:0;font-size:18px;">{banner_text}</h2>
    <p style="margin:4px 0 0;font-size:13px;opacity:0.9;">
      Commit {sha} &middot; Run {run_id} &middot;
      {counts['PASS']} passed, {counts['FAIL']} failed, {counts['ERROR']} errors, {counts['SKIP']} skipped
    </p>
  </div>
{triage_html}
  <table style="width:100%;border-collapse:collapse;margin-top:12px;">
    <tr style="background:#fafafa;">
      <th style="padding:8px 12px;text-align:left;border-bottom:2px solid #ddd;">App</th>
      <th style="padding:8px 12px;text-align:left;border-bottom:2px solid #ddd;">Result</th>
    </tr>
    {''.join(rows)}
  </table>
"""

    if failure_details:
        html += f"""\
  <h3 style="margin-top:24px;color:#c62828;">Failure Details</h3>
  {''.join(failure_details)}
"""

    html += f"""\
  <p style="margin-top:24px;font-size:12px;color:#999;">
    Picodroid {suite} &middot; nightly test run
  </p>
</body>
</html>"""

    return html


def send_email(recipient, subject, html_body, gmail_user, gmail_password):
    """Send an HTML email via Gmail SMTP."""
    msg = MIMEMultipart("alternative")
    msg["From"] = gmail_user
    msg["To"] = recipient
    msg["Subject"] = subject

    # Plain-text fallback.
    plain = subject + "\n\nView this email in an HTML-capable client for the full report."
    msg.attach(MIMEText(plain, "plain"))
    msg.attach(MIMEText(html_body, "html"))

    with smtplib.SMTP(SMTP_HOST, SMTP_PORT) as server:
        server.starttls()
        server.login(gmail_user, gmail_password)
        server.sendmail(gmail_user, [recipient], msg.as_string())
    print(f"Email sent to {recipient}")


def main():
    parser = argparse.ArgumentParser(description="Send picodroid HIL email report")
    parser.add_argument("--results", required=True, help="Path to results summary file")
    parser.add_argument("--log-dir", required=True, help="Directory containing per-app RTT logs")
    parser.add_argument("--run-id", required=True, help="Run identifier (timestamp-sha)")
    parser.add_argument("--sha", required=True, help="Git commit SHA")
    parser.add_argument("--suite", default="HIL", help="Test suite name (e.g. HIL, sim)")
    parser.add_argument("--to", default=DEFAULT_RECIPIENT, help="Recipient email address")
    parser.add_argument(
        "--no-send",
        action="store_true",
        help="Print the subject and triage summary instead of sending (no credentials needed)",
    )
    args = parser.parse_args()

    entries = parse_results(args.results)
    history = load_history(args.results)
    new_fails, known_fails, recovered = classify_failures(entries, history)

    counts = {"PASS": 0, "FAIL": 0, "ERROR": 0, "SKIP": 0}
    for status, _ in entries:
        counts[status] = counts.get(status, 0) + 1
    total_run = counts["PASS"] + counts["FAIL"] + counts["ERROR"]

    all_passed = counts["FAIL"] == 0 and counts["ERROR"] == 0
    suite = args.suite.upper()
    # Subject triage: a fresh regression must read differently from a suite
    # that has been red for weeks — and a recovery is worth celebrating.
    if new_fails:
        status_str = f"{len(new_fails)} NEW FAIL"
    elif not all_passed:
        status_str = "FAIL (known)"
    elif recovered:
        status_str = f"PASS ({len(recovered)} recovered)"
    else:
        status_str = "PASS"
    subject = f"[picodroid {suite}] {status_str}: {counts['PASS']}/{total_run} passed ({args.sha})"

    if args.no_send:
        print(subject)
        for app in new_fails:
            print(f"  NEW FAIL:  {app}")
        for app, (since, open_ended) in sorted(known_fails.items()):
            print(f"  KNOWN:     {app} — {format_known_since(since, open_ended)}")
        for app in recovered:
            print(f"  RECOVERED: {app}")
        return

    gmail_user, gmail_password = load_credentials()
    recipient = args.to if args.to else gmail_user
    triage_html = build_triage_html(new_fails, known_fails, recovered)
    html = build_html(
        entries, args.log_dir, args.run_id, args.sha, suite=suite, triage_html=triage_html
    )
    send_email(recipient, subject, html, gmail_user, gmail_password)


if __name__ == "__main__":
    main()
