---
title: "Protocol Buffers"
description: "CodedInputStream, CodedOutputStream and the generated message classes of protoc-gen-picodroid, over a native wire codec."
---

`picodroid.protobuf.*` — `CodedInputStream`, `CodedOutputStream`, `MessageLite`, `WireFormat` and `InvalidProtocolBufferException` with the method names, signatures and semantics of `com.google.protobuf` (protobuf-javalite; the picodroid namespace rule makes it `picodroid.protobuf`). A `.proto` file becomes Java message classes over these streams with `protoc-gen-picodroid` (`tools/protoc-gen-picodroid/`). See [Java API overview](/api/) for the full API index.

Protobuf is a board capability, like JSON: a board opts in with `has_protobuf = true` in its [`board.toml`](/reference/porting-guide/#boardtoml-reference). Every RP2350 board ships it. `testbench_rp2040` leaves it off, which drops the five classes from that board's embedded SDK and compiles the native codec out, so the board pays nothing for it — and an app that references `picodroid.protobuf` fails that board's `verifyApiContract` at build time (`EXCLUDED ON BOARD testbench_rp2040`) rather than on the device.

## Quick example

With `examples/claudeusage/proto/usage.proto` compiled by `scripts/gen-proto.sh`, the display reads the bridge's reply in one call:

```java
import claudeusage.proto.UsageReply;
import picodroid.protobuf.InvalidProtocolBufferException;

try {
  UsageReply reply = UsageReply.parseFrom(buf, 0, total);   // unknown fields are skipped
  int pct = reply.getSession().getPct();
  for (int i = 0; i < reply.getModelCapsCount(); i++) {
    String name = reply.getModelCaps(i).getName();
  }
  byte[] again = reply.toByteArray();                        // getSerializedSize() bytes
} catch (InvalidProtocolBufferException e) {
  // truncated input, a malformed varint, a zero field number, a group
}
```

The streams work on their own too, the way nanopb's callbacks or javalite's generated code do:

```java
CodedInputStream in = CodedInputStream.newInstance(bytes);
int tag;
while ((tag = in.readTag()) != 0) {
  switch (WireFormat.getTagFieldNumber(tag)) {
    case 1: version = in.readUInt32(); break;
    case 2: name = in.readString(); break;
    default: in.skipField(tag);
  }
}

byte[] out = new byte[CodedOutputStream.computeUInt32Size(1, 7) + CodedOutputStream.computeStringSize(2, "pico")];
CodedOutputStream os = CodedOutputStream.newInstance(out);
os.writeUInt32(1, 7);
os.writeString(2, "pico");
os.checkNoSpaceLeft();
```

## How it works

The byte-at-a-time work — tags, varints, fixed-width values, skipping an unknown field — is native (the `micropb` codec reading the Java `byte[]` in place); slicing a string or a `byte[]` out of the buffer, zigzag, limits and size arithmetic are Java. A parse allocates only what the message holds: no wrappers, no boxing, no pool. `readDouble`/`writeDouble` are native as well because the runtime has no `Double.longBitsToDouble`.

A generated message is a plain mutable class: private fields with proto3 defaults, `getX()`, `hasX()` for message and `optional` fields, `setX(v)`, `clearX()`, repeated fields as primitive or typed arrays behind `getXCount()`/`getX(int)`/`addX(v)`, `static parseFrom(byte[])`/`parseFrom(byte[], int, int)`/`parseFrom(CodedInputStream)`, `mergeFrom(CodedInputStream)`, `writeTo`, `getSerializedSize`, `toByteArray`. Each message is one class, which is what it costs on the device: about 1–2 KB of class metadata per message type.

## Deviations from protobuf-javalite

- Messages are mutable and have setters; there are no `Builder`s (a Builder doubles the class count) and no `equals`/`hashCode`.
- Enums are `int` constants in a constants-only class named after the enum (`Demo.Mood.HAPPY`; javac inlines them, so the class never loads), with `getXValue()`/`setXValue(int)` and, for repeated enums, `getXsCount()`/`getXsValue(i)`/`addXsValue(v)`; no Java `enum` (one class each).
- `bytes` fields and `readBytes` are `byte[]`; there is no `ByteString`.
- No `InputStream`/`OutputStream` sources or sinks (the runtime has neither), no extension registries, no `map`, `oneof`, groups, `Any`, well-known types, reflection or text/JSON formats. proto3 syntax only.
- `readString` does not validate UTF-8: the runtime's strings are byte strings.
- A `CodedOutputStream` that runs out of space throws `OutOfSpaceException` after the bytes that did fit, as javalite does.
