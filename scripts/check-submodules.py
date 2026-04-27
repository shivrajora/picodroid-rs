#!/usr/bin/env python3
"""Daily submodule-version drift checker.

Compares each git submodule's pinned tag against upstream's latest GitHub
release/tag. Emails a summary if any submodule is behind, otherwise stays
silent (cron sees stdout only).

Reuses the Gmail SMTP credentials at ~/.config/picodroid/hil-email.conf
(same file hil-email.py uses).
"""

import argparse
import configparser
import io
import re
import smtplib
import subprocess
import sys
from email.mime.multipart import MIMEMultipart
from email.mime.text import MIMEText
from pathlib import Path

CONF_PATH = Path.home() / ".config" / "picodroid" / "hil-email.conf"
SMTP_HOST = "smtp.gmail.com"
SMTP_PORT = 587

REPO_ROOT = Path(__file__).resolve().parent.parent
GITMODULES = REPO_ROOT / ".gitmodules"


def log(msg):
    print(msg, flush=True)


def load_credentials():
    if not CONF_PATH.exists():
        log(f"ERROR: credentials file not found: {CONF_PATH}")
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
        log(f"ERROR: GMAIL_USER and GMAIL_APP_PASSWORD must be set in {CONF_PATH}")
        sys.exit(1)
    return user, password


def parse_gitmodules():
    """Return list of (name, path, owner, repo) for each submodule."""
    parser = configparser.ConfigParser()
    parser.read(GITMODULES)

    out = []
    for section in parser.sections():
        # section is like 'submodule "vendor/lvgl"'
        m = re.match(r'submodule "(.+)"$', section)
        if not m:
            continue
        name = m.group(1)
        path = parser[section].get("path", name)
        url = parser[section].get("url", "")
        gh = re.match(r"https://github\.com/([^/]+)/([^/.]+)(?:\.git)?/?$", url)
        if not gh:
            log(f"  skip {name}: not a github URL ({url})")
            continue
        owner, repo = gh.group(1), gh.group(2)
        out.append((name, path, owner, repo))
    return out


def pinned_tag(path):
    """Return the closest tag at the submodule's pinned commit, or None."""
    try:
        result = subprocess.run(
            ["git", "-C", str(REPO_ROOT / path), "describe", "--tags", "--abbrev=0", "HEAD"],
            capture_output=True, text=True, check=True,
        )
        return result.stdout.strip()
    except FileNotFoundError:
        log("  ERROR: git not on PATH")
        return None
    except subprocess.CalledProcessError as e:
        log(f"  pinned_tag({path}) failed: {e.stderr.strip()}")
        return None


def latest_upstream(owner, repo):
    """Return the upstream tag name. Tries /releases/latest, falls back to /tags."""
    try:
        result = subprocess.run(
            ["gh", "api", f"repos/{owner}/{repo}/releases/latest", "--jq", ".tag_name"],
            capture_output=True, text=True, check=True,
        )
        return result.stdout.strip()
    except FileNotFoundError:
        log("  ERROR: gh not on PATH")
        return None
    except subprocess.CalledProcessError:
        # 404 (no releases) → fall back to most-recent tag
        try:
            result = subprocess.run(
                ["gh", "api", f"repos/{owner}/{repo}/tags", "--jq", ".[0].name"],
                capture_output=True, text=True, check=True,
            )
            return result.stdout.strip()
        except subprocess.CalledProcessError as e:
            log(f"  latest_upstream({owner}/{repo}) failed: {e.stderr.strip()}")
            return None


def build_html(rows, all_count):
    """rows: list of dicts with name, pinned, latest, behind (bool)."""
    behind = [r for r in rows if r["behind"]]
    body = io.StringIO()
    body.write('<html><body style="font-family:-apple-system,sans-serif;font-size:14px">')
    if behind:
        body.write(f"<h2 style='color:#b00'>{len(behind)} submodule(s) behind upstream</h2>")
    else:
        body.write(f"<h2 style='color:#080'>All {all_count} submodules up to date</h2>")
    body.write('<table cellpadding="6" cellspacing="0" border="1" style="border-collapse:collapse">')
    body.write("<tr><th>Submodule</th><th>Pinned</th><th>Latest upstream</th><th>Status</th></tr>")
    for r in rows:
        color = "#fee" if r["behind"] else "#efe"
        status = "BEHIND" if r["behind"] else "ok"
        body.write(
            f'<tr style="background:{color}">'
            f'<td>{r["name"]}</td>'
            f'<td>{r["pinned"] or "?"}</td>'
            f'<td>{r["latest"] or "?"}</td>'
            f'<td>{status}</td></tr>'
        )
    body.write("</table>")
    body.write("<p style='color:#888;font-size:11px'>Picodroid &middot; daily submodule check</p>")
    body.write("</body></html>")
    return body.getvalue()


def send_email(recipient, subject, html_body, gmail_user, gmail_password):
    msg = MIMEMultipart("alternative")
    msg["From"] = gmail_user
    msg["To"] = recipient
    msg["Subject"] = subject
    plain = subject + "\n\nView this email in an HTML-capable client for the full report."
    msg.attach(MIMEText(plain, "plain"))
    msg.attach(MIMEText(html_body, "html"))
    with smtplib.SMTP(SMTP_HOST, SMTP_PORT) as server:
        server.starttls()
        server.login(gmail_user, gmail_password)
        server.sendmail(gmail_user, [recipient], msg.as_string())
    log(f"Email sent to {recipient}")


def main():
    p = argparse.ArgumentParser(description="Check submodule versions against upstream")
    p.add_argument("--dry-run", action="store_true", help="Print the email body instead of sending")
    p.add_argument("--always-email", action="store_true", help="Send mail even when all up to date")
    p.add_argument("--to", default=None, help="Recipient address (default: GMAIL_USER)")
    p.add_argument(
        "--pretend-behind",
        action="append",
        default=[],
        metavar="path=tag",
        help="Force a submodule to look pinned at TAG (debug; repeatable)",
    )
    args = p.parse_args()

    pretend = {}
    for spec in args.pretend_behind:
        path, _, tag = spec.partition("=")
        if not path or not tag:
            log(f"  ignoring malformed --pretend-behind '{spec}'")
            continue
        pretend[path] = tag

    submodules = parse_gitmodules()
    rows = []
    for name, path, owner, repo in submodules:
        pinned = pretend.get(path) or pretend.get(name) or pinned_tag(path)
        latest = latest_upstream(owner, repo)
        behind = bool(pinned and latest and pinned != latest)
        if not pinned or not latest:
            status = "unknown"
        elif behind:
            status = "BEHIND"
        else:
            status = "ok"
        log(f"  {name}: pinned={pinned} latest={latest} {status}")
        rows.append({"name": name, "pinned": pinned, "latest": latest, "behind": behind})

    behind_count = sum(1 for r in rows if r["behind"])
    if behind_count == 0 and not args.always_email:
        log("All submodules up to date — no email sent.")
        return

    subject = (
        f"[picodroid] {behind_count} submodule(s) behind upstream"
        if behind_count > 0
        else "[picodroid] All submodules up to date"
    )
    html = build_html(rows, all_count=len(rows))

    if args.dry_run:
        log(f"--- dry-run subject ---\n{subject}")
        log(f"--- dry-run html ---\n{html}")
        return

    gmail_user, gmail_password = load_credentials()
    recipient = args.to or gmail_user
    send_email(recipient, subject, html, gmail_user, gmail_password)


if __name__ == "__main__":
    main()
