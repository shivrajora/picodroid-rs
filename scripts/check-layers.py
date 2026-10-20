#!/usr/bin/env python3
"""Crate dependency direction against scripts/layers.conf.

A workspace crate may depend only on crates in a strictly lower layer (L0 is
the bottom). The `host` layer (build-time and host tools) may use host crates
and L0..L3, and anyone may name a host crate as a build- or dev-dependency;
the `test` layer may use anything and may only be a dev-dependency. The graph
comes from `cargo metadata --no-deps`, so the check needs cargo and nothing
else, and never touches the network.

    ./scripts/check-layers.py            # exit 1 with every violation listed
    ./scripts/check-layers.py --print    # the graph, one crate per line
"""

import json
import pathlib
import subprocess
import sys

REPO_ROOT = pathlib.Path(__file__).resolve().parent.parent
CONF = REPO_ROOT / "scripts" / "layers.conf"
HOST_MAX = 3  # a host crate may depend on L0..L3


def read_conf():
    layers = {}
    for line in CONF.read_text().splitlines():
        line = line.split("#", 1)[0].strip()
        if not line:
            continue
        path, layer = line.split()
        layers[path] = layer
    return layers


def metadata():
    out = subprocess.run(
        ["cargo", "metadata", "--format-version", "1", "--no-deps"],
        cwd=REPO_ROOT, check=True, capture_output=True, text=True,
    ).stdout
    meta = json.loads(out)
    members = set(meta["workspace_members"])
    pkgs = [p for p in meta["packages"] if p["id"] in members]
    by_name = {}
    for p in pkgs:
        rel = pathlib.Path(p["manifest_path"]).parent.relative_to(REPO_ROOT)
        by_name[p["name"]] = (str(rel), p)
    return by_name


def rank(layer):
    return int(layer[1:]) if layer.startswith("L") else None


def allowed(crate_layer, dep_layer, kind):
    """Why a dependency is allowed, or None."""
    if crate_layer == "test":
        return "test crates may use anything"
    if dep_layer == "test":
        return "dev-dependency on a test crate" if kind == "dev" else None
    if dep_layer == "host":
        if crate_layer == "host" or kind in ("build", "dev"):
            return "host crate as a %s dependency" % (kind or "host")
        return None
    dep_rank = rank(dep_layer)
    if crate_layer == "host":
        return "host may use L0..L%d" % HOST_MAX if dep_rank <= HOST_MAX else None
    return "lower layer" if dep_rank < rank(crate_layer) else None


def main(argv):
    layers = read_conf()
    by_name = metadata()
    problems = []
    seen_paths = set()
    for name, (path, pkg) in sorted(by_name.items(), key=lambda kv: kv[1][0]):
        seen_paths.add(path)
        if path not in layers:
            problems.append("%s (%s): not in scripts/layers.conf; register it" % (path, name))
            continue
        layer = layers[path]
        for dep in pkg["dependencies"]:
            if dep["name"] not in by_name:
                continue
            dep_path = by_name[dep["name"]][0]
            dep_layer = layers.get(dep_path)
            if dep_layer is None:
                continue  # reported on its own line above
            kind = dep["kind"] or "normal"
            if "--print" in argv:
                print("%-24s %-5s -> %-24s %-5s %s" % (path, layer, dep_path, dep_layer, kind))
            if allowed(layer, dep_layer, kind) is None:
                problems.append("%s (%s) -> %s (%s) as a %s dependency" %
                                (path, layer, dep_path, dep_layer, kind))
    for path in sorted(set(layers) - seen_paths):
        problems.append("%s: in scripts/layers.conf but not a workspace member" % path)
    if not problems:
        return 0
    print("")
    for p in problems:
        print("         " + p)
    print("")
    print("ERROR: a crate depends on a crate in the same or a higher layer")
    print("       (scripts/layers.conf; the map is in CLAUDE.md). Move the shared")
    print("       code down a layer, or move the dependent crate up; a new crate")
    print("       is registered in layers.conf with the layer its dependencies allow.")
    return 1


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
