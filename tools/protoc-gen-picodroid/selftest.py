#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-only
"""Run protoc-gen-picodroid over fixtures/ and compare with golden/.

`--update` rewrites golden/ instead of comparing. Needs the toolchain in
requirements.txt on the interpreter running this script (or on PATH ahead of
the system python, since protoc launches the plugin by its shebang).
"""

import os
import shutil
import subprocess
import sys
import tempfile

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(os.path.dirname(HERE))
PLUGIN = os.path.join(HERE, "protoc_gen_picodroid.py")
FIXTURES = os.path.join(HERE, "fixtures")
GOLDEN = os.path.join(HERE, "golden")
JAR = os.path.join(ROOT, "third_party", "google-java-format-1.36.1-all-deps.jar")


def generate(out_dir):
    env = dict(os.environ)
    env["PATH"] = os.path.dirname(sys.executable) + os.pathsep + env.get("PATH", "")
    for name in sorted(os.listdir(FIXTURES)):
        if not name.endswith(".proto"):
            continue
        subprocess.run(
            [
                sys.executable,
                "-m",
                "grpc_tools.protoc",
                "-I" + FIXTURES,
                "--plugin=protoc-gen-picodroid=" + PLUGIN,
                "--picodroid_out=" + out_dir,
                name,
            ],
            check=True,
            env=env,
        )
    javas = [
        os.path.join(dp, f) for dp, _, fs in os.walk(out_dir) for f in fs if f.endswith(".java")
    ]
    if os.path.exists(JAR) and javas:
        subprocess.run(["java", "-jar", JAR, "--replace"] + sorted(javas), check=True)
    return {
        os.path.relpath(p, out_dir): open(p, encoding="utf-8").read() for p in javas
    }


def main():
    update = "--update" in sys.argv[1:]
    with tempfile.TemporaryDirectory() as tmp:
        fresh = generate(tmp)
        if update:
            shutil.rmtree(GOLDEN, ignore_errors=True)
            for rel, text in fresh.items():
                dst = os.path.join(GOLDEN, rel)
                os.makedirs(os.path.dirname(dst), exist_ok=True)
                with open(dst, "w", encoding="utf-8") as f:
                    f.write(text)
            print("wrote %d golden file(s)" % len(fresh))
            return 0
        golden = {}
        for dp, _, fs in os.walk(GOLDEN):
            for f in fs:
                p = os.path.join(dp, f)
                golden[os.path.relpath(p, GOLDEN)] = open(p, encoding="utf-8").read()
        bad = sorted(set(fresh) ^ set(golden)) + sorted(
            k for k in fresh if k in golden and fresh[k] != golden[k]
        )
        if bad:
            print("protoc-gen-picodroid: golden output differs:", file=sys.stderr)
            for k in bad:
                print("  " + k, file=sys.stderr)
            print("  (run selftest.py --update if the change is intended)", file=sys.stderr)
            return 1
        print("protoc-gen-picodroid: %d golden file(s) match" % len(golden))
        return 0


if __name__ == "__main__":
    sys.exit(main())
