// SPDX-License-Identifier: GPL-3.0-only
package protodemo;

import java.io.IOException;
import picodroid.app.Application;
import picodroid.protobuf.CodedInputStream;
import picodroid.protobuf.CodedOutputStream;
import picodroid.protobuf.InvalidProtocolBufferException;
import picodroid.protobuf.WireFormat;
import picodroid.util.Log;
import protodemo.proto.Demo;

/**
 * End-to-end checks of {@code picodroid.protobuf} and {@code protoc-gen-picodroid}: a message with
 * every field kind round-trips through {@code toByteArray}/{@code parseFrom} byte for byte, unknown
 * fields of every wire type are skipped, truncated and malformed input throws {@link
 * InvalidProtocolBufferException} (the native codec raising a Java exception), packed and unpacked
 * repeated fields read the same, the streams agree with their own size arithmetic, and 300 parses
 * leave the heap alone. Logs one PASS/FAIL line per check and {@code === PASSED ===} when all hold.
 * Needs a board with {@code has_protobuf = true}.
 */
public class ProtoDemo extends Application {
  private static final String TAG = "ProtoDemo";
  private static int fails = 0;

  private static void check(String what, boolean ok) {
    Log.i(TAG, (ok ? "PASS: " : "FAIL: ") + what);
    if (!ok) {
      fails = fails + 1;
    }
  }

  @Override
  public void onCreate() {
    Log.i(TAG, "=== ProtoDemo start ===");
    try {
      runChecks();
    } catch (Throwable e) {
      check("no exception escaped the checks: " + e, false);
    }
    if (fails == 0) {
      Log.i(TAG, "=== PASSED ===");
    } else {
      Log.i(TAG, "=== FAILED (" + fails + ") ===");
    }
  }

  private void runChecks() throws Exception {
    roundTripChecks();
    defaultChecks();
    unknownFieldChecks();
    errorChecks();
    packedChecks();
    streamChecks();
    sizeChecks();
    churnChecks();
  }

  // ── a message with everything in it ──────────────────────────────────────

  static Demo.Everything sample() {
    Demo.Everything e = new Demo.Everything();
    e.setD(1.5).setF(-2.25f).setI32(-5).setI64(Long.MIN_VALUE).setU32(0xFFFFFFFF).setU64(-1L);
    e.setS32(-1).setS64(Long.MAX_VALUE).setF32(0xDEADBEEF).setF64(0x0102030405060708L);
    e.setSf32(-7).setSf64(-8L).setFlag(true).setName("pico").setBlob(new byte[] {1, 2, (byte) 250});
    e.setMoodValue(Demo.Mood.GRUMPY).setKindValue(Demo.Everything.Kind.SQUARE);
    Demo.Everything.Inner inner = new Demo.Everything.Inner().setLabel("in");
    inner.addCorners(new Demo.Point().setX(-1).setY(1));
    inner.addCorners(new Demo.Point().setX(300).setY(-300));
    e.setInner(inner);
    e.setMaybe(0).setMaybeName("");
    e.addCounts(1).addCounts(-1).addCounts(300).addCounts(Integer.MIN_VALUE);
    e.addFlags(true).addFlags(false).addFlags(true);
    e.addTags("a").addTags("").addTags("ccc");
    e.addBlobs(new byte[0]).addBlobs(new byte[] {9});
    e.addPoints(new Demo.Point().setX(5).setY(6));
    e.addMoodsValue(Demo.Mood.HAPPY).addMoodsValue(Demo.Mood.GRUMPY);
    e.addWeights(0.5).addWeights(-1e300);
    e.setClass_(28).setTwoWordsHere(29);
    return e;
  }

  static boolean same(Demo.Point a, Demo.Point b) {
    return a.getX() == b.getX() && a.getY() == b.getY();
  }

  static boolean sameBytes(byte[] a, byte[] b) {
    if (a.length != b.length) {
      return false;
    }
    for (int i = 0; i < a.length; i++) {
      if (a[i] != b[i]) {
        return false;
      }
    }
    return true;
  }

  private static void roundTripChecks() throws IOException {
    Demo.Everything e = sample();
    byte[] bytes = e.toByteArray();
    check(
        "getSerializedSize matches toByteArray (" + bytes.length + " B)",
        e.getSerializedSize() == bytes.length);
    Demo.Everything p = Demo.Everything.parseFrom(bytes);
    check("double", p.getD() == 1.5);
    check("float", p.getF() == -2.25f);
    check("negative int32 (ten-byte varint)", p.getI32() == -5);
    check("int64 min", p.getI64() == Long.MIN_VALUE);
    check("uint32 all ones", p.getU32() == 0xFFFFFFFF);
    check("uint64 all ones", p.getU64() == -1L);
    check("sint32 -1 (zigzag)", p.getS32() == -1);
    check("sint64 max", p.getS64() == Long.MAX_VALUE);
    check("fixed32", p.getF32() == 0xDEADBEEF);
    check("fixed64", p.getF64() == 0x0102030405060708L);
    check("sfixed32", p.getSf32() == -7);
    check("sfixed64", p.getSf64() == -8L);
    check("bool", p.getFlag());
    check("string", "pico".equals(p.getName()));
    check("bytes", sameBytes(p.getBlob(), new byte[] {1, 2, (byte) 250}));
    check("enum", p.getMoodValue() == Demo.Mood.GRUMPY && p.getKindValue() == 7);
    check("nested message", p.hasInner() && "in".equals(p.getInner().getLabel()));
    check(
        "repeated message inside a nested message",
        p.getInner().getCornersCount() == 2
            && same(p.getInner().getCorners(0), new Demo.Point().setX(-1).setY(1))
            && same(p.getInner().getCorners(1), new Demo.Point().setX(300).setY(-300)));
    check("optional int32 set to its default is present", p.hasMaybe() && p.getMaybe() == 0);
    check(
        "optional string set to empty is present", p.hasMaybeName() && "".equals(p.getMaybeName()));
    check(
        "packed repeated int32",
        p.getCountsCount() == 4
            && p.getCounts(0) == 1
            && p.getCounts(1) == -1
            && p.getCounts(2) == 300
            && p.getCounts(3) == Integer.MIN_VALUE);
    check(
        "packed repeated bool",
        p.getFlagsCount() == 3 && p.getFlags(0) && !p.getFlags(1) && p.getFlags(2));
    check(
        "repeated string incl. empty",
        p.getTagsCount() == 3
            && "a".equals(p.getTags(0))
            && "".equals(p.getTags(1))
            && "ccc".equals(p.getTags(2)));
    check(
        "repeated bytes incl. empty",
        p.getBlobsCount() == 2 && p.getBlobs(0).length == 0 && p.getBlobs(1)[0] == 9);
    check(
        "repeated message",
        p.getPointsCount() == 1 && same(p.getPoints(0), new Demo.Point().setX(5).setY(6)));
    check(
        "packed repeated enum",
        p.getMoodsCount() == 2 && p.getMoodsValue(0) == 1 && p.getMoodsValue(1) == 2);
    check(
        "packed repeated double",
        p.getWeightsCount() == 2 && p.getWeights(0) == 0.5 && p.getWeights(1) == -1e300);
    check("a field named `class` is getClass_()", p.getClass_() == 28);
    check("snake_case field names camel-case", p.getTwoWordsHere() == 29);
    byte[] again = p.toByteArray();
    check("re-serializing the parse is byte-identical", sameBytes(bytes, again));
  }

  private static void defaultChecks() throws IOException {
    Demo.Everything empty = new Demo.Everything();
    check("an empty message serializes to nothing", empty.toByteArray().length == 0);
    Demo.Everything p = Demo.Everything.parseFrom(new byte[0]);
    check(
        "defaults after parsing nothing",
        p.getI32() == 0
            && "".equals(p.getName())
            && p.getBlob().length == 0
            && !p.getFlag()
            && !p.hasInner()
            && p.getInner() == null
            && !p.hasMaybe()
            && p.getCountsCount() == 0);
    boolean threw = false;
    try {
      p.getCounts(0);
    } catch (IndexOutOfBoundsException e) {
      threw = true;
    }
    check("a repeated index past the count throws", threw);
    p.setMaybe(3);
    check("setting an optional makes it present", p.hasMaybe());
    p.clearMaybe();
    check("clearing an optional makes it absent", !p.hasMaybe() && p.getMaybe() == 0);
    check(
        "a set-then-cleared message is empty again",
        p.setName("x").clearName().toByteArray().length == 0);
  }

  // ── unknown fields ───────────────────────────────────────────────────────

  private static void unknownFieldChecks() throws IOException {
    byte[] buf = new byte[64];
    CodedOutputStream out = CodedOutputStream.newInstance(buf);
    out.writeInt32(99, 12345); // varint
    out.writeFixed64(98, 1L); // fixed64
    out.writeString(97, "skip me"); // length-delimited
    out.writeFixed32(96, 2); // fixed32
    out.writeInt32(3, 42); // i32: the one field the reader knows
    out.writeBytes(95, new byte[] {1, 2, 3});
    int n = out.getTotalBytesWritten();
    Demo.Everything p = Demo.Everything.parseFrom(buf, 0, n);
    check("unknown fields of every wire type are skipped", p.getI32() == 42);
    check("nothing else was set by the unknown fields", p.toByteArray().length == 2);
  }

  // ── errors ───────────────────────────────────────────────────────────────

  private static boolean invalid(byte[] bytes, int len, String what) {
    try {
      Demo.Everything.parseFrom(bytes, 0, len);
      return false;
    } catch (InvalidProtocolBufferException e) {
      Log.i(TAG, what + ": " + e.getMessage());
      return e.getMessage() != null;
    }
  }

  private static void errorChecks() throws IOException {
    byte[] bytes = sample().toByteArray();
    check("a message cut short throws", invalid(bytes, bytes.length - 1, "truncated"));
    check("a message cut in a tag throws", invalid(bytes, 1, "cut in a tag"));
    byte[] varint = new byte[12];
    varint[0] = 24; // field 3, varint
    for (int i = 1; i < 12; i++) {
      varint[i] = (byte) 0x80;
    }
    check("an eleven-byte varint throws", invalid(varint, 12, "overlong varint"));
    check("a zero tag throws", invalid(new byte[] {0, 1}, 2, "zero tag"));
    check("a group tag throws", invalid(new byte[] {0x0B}, 1, "group"));
    check(
        "a length past the end throws",
        invalid(new byte[] {(byte) 0x72, 100, 1}, 3, "long length"));
    // The native codec raises the exception by class name: it must be catchable as IOException
    // and carry its message.
    boolean asIo = false;
    try {
      CodedInputStream.newInstance(new byte[] {(byte) 0x80}).readRawVarint32();
    } catch (IOException e) {
      asIo =
          e instanceof InvalidProtocolBufferException && e.getMessage().indexOf("truncated") >= 0;
    }
    check("a native-thrown InvalidProtocolBufferException is an IOException with a message", asIo);
    boolean space = false;
    try {
      CodedOutputStream.newInstance(new byte[3]).writeInt32(1, 300);
      CodedOutputStream.newInstance(new byte[2]).writeInt32(1, 300000);
    } catch (CodedOutputStream.OutOfSpaceException e) {
      space = true;
    }
    check("writing past the buffer throws OutOfSpaceException", space);
  }

  // ── packed vs unpacked ───────────────────────────────────────────────────

  private static void packedChecks() throws IOException {
    byte[] buf = new byte[32];
    CodedOutputStream out = CodedOutputStream.newInstance(buf);
    out.writeInt32(21, 7); // counts, one per tag
    out.writeInt32(21, -2);
    out.writeInt32(21, 300);
    int n = out.getTotalBytesWritten();
    Demo.Everything unpacked = Demo.Everything.parseFrom(buf, 0, n);
    Demo.Everything packed = Demo.Everything.parseFrom(unpacked.toByteArray());
    check(
        "unpacked repeated int32 reads like packed",
        unpacked.getCountsCount() == 3
            && unpacked.getCounts(1) == -2
            && packed.getCountsCount() == 3
            && packed.getCounts(2) == 300);
    check("the writer packs repeated scalars", unpacked.toByteArray().length < n);
  }

  // ── the streams on their own ─────────────────────────────────────────────

  private static void streamChecks() throws IOException {
    byte[] buf = new byte[64];
    CodedOutputStream out = CodedOutputStream.newInstance(buf);
    out.writeTag(1, WireFormat.WIRETYPE_VARINT);
    out.writeSInt32NoTag(-1);
    out.writeSInt64(2, -2L);
    out.writeDouble(3, 2.5);
    out.writeString(4, "");
    out.writeBool(5, true);
    out.writeUInt64(6, 1L << 40);
    int n = out.getTotalBytesWritten();
    check("sint32 -1 takes one byte", buf[1] == 1);
    CodedInputStream in = CodedInputStream.newInstance(buf, 0, n);
    boolean ok = in.readTag() == 8 && in.readSInt32() == -1;
    ok = ok && in.readTag() == 16 && in.readSInt64() == -2L;
    ok = ok && in.readTag() == 25 && in.readDouble() == 2.5;
    ok = ok && in.readTag() == 34 && "".equals(in.readString());
    ok = ok && in.readTag() == 40 && in.readBool();
    ok = ok && in.readTag() == 48 && in.readUInt64() == (1L << 40);
    ok = ok && in.readTag() == 0 && in.isAtEnd() && in.getTotalBytesRead() == n;
    check("the input stream reads back what the output stream wrote", ok);
    in = CodedInputStream.newInstance(buf, 0, n);
    in.readTag();
    int old = in.pushLimit(1);
    boolean limited = in.readSInt32() == -1 && in.readTag() == 0 && in.getBytesUntilLimit() == 0;
    in.popLimit(old);
    limited = limited && in.readTag() == 16;
    check("pushLimit/popLimit narrow and restore the stream", limited);
    check(
        "WireFormat splits tags",
        WireFormat.getTagFieldNumber(48) == 6
            && WireFormat.getTagWireType(25) == WireFormat.WIRETYPE_FIXED64
            && WireFormat.makeTag(6, 0) == 48);
  }

  private static void sizeChecks() {
    boolean ok = CodedOutputStream.computeInt32SizeNoTag(-1) == 10;
    ok = ok && CodedOutputStream.computeUInt32SizeNoTag(300) == 2;
    ok = ok && CodedOutputStream.computeUInt64SizeNoTag(-1L) == 10;
    ok = ok && CodedOutputStream.computeSInt32SizeNoTag(-1) == 1;
    ok =
        ok
            && CodedOutputStream.computeTagSize(15) == 1
            && CodedOutputStream.computeTagSize(16) == 2;
    ok = ok && CodedOutputStream.computeStringSize(1, "pico") == 6;
    ok = ok && CodedOutputStream.computeMessageSize(1, new Demo.Point().setX(-1)) == 4;
    check("size arithmetic", ok);
  }

  private static void churnChecks() throws IOException {
    byte[] bytes = sample().toByteArray();
    for (int i = 0; i < 300; i++) {
      Demo.Everything p = Demo.Everything.parseFrom(bytes);
      if (p.getCountsCount() != 4 || p.getInner().getCornersCount() != 2) {
        check("parse in iteration " + i, false);
        return;
      }
    }
    check("300 parses of the sample", true);
  }
}
