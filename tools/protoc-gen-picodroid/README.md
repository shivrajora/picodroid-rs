# protoc-gen-picodroid

A `protoc` plugin that turns a proto3 `.proto` file into Java message classes over
`picodroid.protobuf` (`CodedInputStream` / `CodedOutputStream`), the subset of protobuf-javalite
this runtime carries. The shape of the generated code and its deviations from javalite are
documented in `website/src/content/docs/api/protobuf.md`.

## Use

```bash
pip install -r tools/protoc-gen-picodroid/requirements.txt   # or: uv pip install -r ...
./scripts/gen-proto.sh            # regenerate every examples/*/proto/*.proto
./scripts/gen-proto.sh --check    # what pre-commit and CI run: fail if the committed output is stale
```

An app keeps its schema in `examples/<app>/proto/<name>.proto`; the Java lands under
`examples/<app>/java/` (in the package `option java_package` names) and, when the app has a
`bridge/` directory, protoc's own Python output lands there as `<name>_pb2.py`. Generated files are
committed — the build does not need `protoc` — and `--check` keeps them honest.

`protoc` runs the plugin through its `#!/usr/bin/env python3` line, so the Python that has
`grpc_tools` installed must be first on `PATH`; `gen-proto.sh` arranges that from
`PICODROID_PROTO_PYTHON` (default `python3`).

## Test

```bash
python3 tools/protoc-gen-picodroid/selftest.py            # fixtures/all_kinds.proto against golden/
python3 tools/protoc-gen-picodroid/selftest.py --update   # accept a deliberate change
```

`examples/protodemo` round-trips every field kind on the runtime itself (nightly sim row).
