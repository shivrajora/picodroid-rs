// SPDX-License-Identifier: GPL-3.0-only
package picodroid.protobuf;

import java.io.IOException;

/**
 * Writes protobuf wire-format values into a {@code byte[]}, with the method names and semantics of
 * {@code com.google.protobuf.CodedOutputStream}: {@code writeXxx(fieldNumber, value)} writes a tag
 * and a value, the {@code NoTag} variants a bare value, and the static {@code computeXxxSize}
 * helpers say how many bytes each takes so a message can size its buffer first.
 *
 * <p>Available only on boards whose board.toml sets {@code has_protobuf = true}. Varints and
 * fixed-width values are written by the native codec; strings, byte arrays and embedded messages
 * are copied in Java. A value that does not fit throws {@link OutOfSpaceException} after the bytes
 * that did fit, as javalite does. Deviations: no {@code ByteString}, no {@code OutputStream} sinks,
 * no groups.
 */
public final class CodedOutputStream {
  /** Thrown when a write would pass the end of the buffer. */
  public static class OutOfSpaceException extends IOException {
    public OutOfSpaceException() {
      super("CodedOutputStream was writing to a flat byte array and ran out of space.");
    }
  }

  // Slot-addressed from Rust (native_handler/protobuf/fields.rs): these instance fields stay in
  // this order, with nothing declared before them.
  private final byte[] mBuf;
  private int mPos;
  private final int mLimit;
  private final int mStart;

  private CodedOutputStream(byte[] buf, int off, int len) {
    mBuf = buf;
    mPos = off;
    mLimit = off + len;
    mStart = off;
  }

  /** A stream over all of {@code buf}. */
  public static CodedOutputStream newInstance(byte[] buf) {
    return newInstance(buf, 0, buf.length);
  }

  /** A stream over {@code len} bytes of {@code buf} starting at {@code off}. */
  public static CodedOutputStream newInstance(byte[] buf, int off, int len) {
    if (buf == null) {
      throw new NullPointerException("buf == null");
    }
    if (off < 0 || len < 0 || off + len > buf.length) {
      throw new ArrayIndexOutOfBoundsException(off + len);
    }
    return new CodedOutputStream(buf, off, len);
  }

  // ── tagged writes ────────────────────────────────────────────────────────

  public void writeTag(int fieldNumber, int wireType) throws IOException {
    writeUInt32NoTag(WireFormat.makeTag(fieldNumber, wireType));
  }

  public void writeInt32(int fieldNumber, int value) throws IOException {
    writeTag(fieldNumber, WireFormat.WIRETYPE_VARINT);
    writeInt32NoTag(value);
  }

  public void writeUInt32(int fieldNumber, int value) throws IOException {
    writeTag(fieldNumber, WireFormat.WIRETYPE_VARINT);
    writeUInt32NoTag(value);
  }

  public void writeSInt32(int fieldNumber, int value) throws IOException {
    writeTag(fieldNumber, WireFormat.WIRETYPE_VARINT);
    writeSInt32NoTag(value);
  }

  public void writeFixed32(int fieldNumber, int value) throws IOException {
    writeTag(fieldNumber, WireFormat.WIRETYPE_FIXED32);
    writeFixed32NoTag(value);
  }

  public void writeSFixed32(int fieldNumber, int value) throws IOException {
    writeTag(fieldNumber, WireFormat.WIRETYPE_FIXED32);
    writeFixed32NoTag(value);
  }

  public void writeInt64(int fieldNumber, long value) throws IOException {
    writeTag(fieldNumber, WireFormat.WIRETYPE_VARINT);
    writeInt64NoTag(value);
  }

  public void writeUInt64(int fieldNumber, long value) throws IOException {
    writeTag(fieldNumber, WireFormat.WIRETYPE_VARINT);
    writeUInt64NoTag(value);
  }

  public void writeSInt64(int fieldNumber, long value) throws IOException {
    writeTag(fieldNumber, WireFormat.WIRETYPE_VARINT);
    writeSInt64NoTag(value);
  }

  public void writeFixed64(int fieldNumber, long value) throws IOException {
    writeTag(fieldNumber, WireFormat.WIRETYPE_FIXED64);
    writeFixed64NoTag(value);
  }

  public void writeSFixed64(int fieldNumber, long value) throws IOException {
    writeTag(fieldNumber, WireFormat.WIRETYPE_FIXED64);
    writeFixed64NoTag(value);
  }

  public void writeBool(int fieldNumber, boolean value) throws IOException {
    writeTag(fieldNumber, WireFormat.WIRETYPE_VARINT);
    writeBoolNoTag(value);
  }

  public void writeEnum(int fieldNumber, int value) throws IOException {
    writeTag(fieldNumber, WireFormat.WIRETYPE_VARINT);
    writeInt32NoTag(value);
  }

  public void writeFloat(int fieldNumber, float value) throws IOException {
    writeTag(fieldNumber, WireFormat.WIRETYPE_FIXED32);
    writeFloatNoTag(value);
  }

  public void writeDouble(int fieldNumber, double value) throws IOException {
    writeTag(fieldNumber, WireFormat.WIRETYPE_FIXED64);
    writeDoubleNoTag(value);
  }

  public void writeString(int fieldNumber, String value) throws IOException {
    writeTag(fieldNumber, WireFormat.WIRETYPE_LENGTH_DELIMITED);
    writeStringNoTag(value);
  }

  public void writeBytes(int fieldNumber, byte[] value) throws IOException {
    writeTag(fieldNumber, WireFormat.WIRETYPE_LENGTH_DELIMITED);
    writeBytesNoTag(value);
  }

  public void writeMessage(int fieldNumber, MessageLite value) throws IOException {
    writeTag(fieldNumber, WireFormat.WIRETYPE_LENGTH_DELIMITED);
    writeMessageNoTag(value);
  }

  // ── bare writes ──────────────────────────────────────────────────────────

  /** An {@code int32}: negative values take ten bytes, as on every protobuf runtime. */
  public void writeInt32NoTag(int value) throws IOException {
    check(nativeWriteVarint64(this, (long) value));
  }

  public void writeUInt32NoTag(int value) throws IOException {
    check(nativeWriteVarint32(this, value));
  }

  public void writeSInt32NoTag(int value) throws IOException {
    check(nativeWriteVarint32(this, encodeZigZag32(value)));
  }

  public void writeFixed32NoTag(int value) throws IOException {
    check(nativeWriteFixed32(this, value));
  }

  public void writeSFixed32NoTag(int value) throws IOException {
    check(nativeWriteFixed32(this, value));
  }

  public void writeInt64NoTag(long value) throws IOException {
    check(nativeWriteVarint64(this, value));
  }

  public void writeUInt64NoTag(long value) throws IOException {
    check(nativeWriteVarint64(this, value));
  }

  public void writeSInt64NoTag(long value) throws IOException {
    check(nativeWriteVarint64(this, encodeZigZag64(value)));
  }

  public void writeFixed64NoTag(long value) throws IOException {
    check(nativeWriteFixed64(this, value));
  }

  public void writeSFixed64NoTag(long value) throws IOException {
    check(nativeWriteFixed64(this, value));
  }

  public void writeBoolNoTag(boolean value) throws IOException {
    writeRawByte(value ? 1 : 0);
  }

  public void writeEnumNoTag(int value) throws IOException {
    writeInt32NoTag(value);
  }

  public void writeFloatNoTag(float value) throws IOException {
    check(nativeWriteFixed32(this, Float.floatToIntBits(value)));
  }

  public void writeDoubleNoTag(double value) throws IOException {
    check(nativeWriteDouble(this, value));
  }

  /** A length-prefixed string, as the runtime's bytes. */
  public void writeStringNoTag(String value) throws IOException {
    byte[] bytes = value.getBytes();
    writeUInt32NoTag(bytes.length);
    writeRawBytes(bytes, 0, bytes.length);
  }

  public void writeBytesNoTag(byte[] value) throws IOException {
    writeUInt32NoTag(value.length);
    writeRawBytes(value, 0, value.length);
  }

  /** A length-prefixed embedded message: its size, then its fields. */
  public void writeMessageNoTag(MessageLite value) throws IOException {
    writeUInt32NoTag(value.getSerializedSize());
    value.writeTo(this);
  }

  /** Alias of {@link #writeUInt32NoTag}, javalite's older name. */
  public void writeRawVarint32(int value) throws IOException {
    writeUInt32NoTag(value);
  }

  /** Alias of {@link #writeUInt64NoTag}, javalite's older name. */
  public void writeRawVarint64(long value) throws IOException {
    writeUInt64NoTag(value);
  }

  public void writeRawByte(int value) throws IOException {
    if (mPos == mLimit) {
      throw new OutOfSpaceException();
    }
    mBuf[mPos++] = (byte) value;
  }

  public void writeRawBytes(byte[] value) throws IOException {
    writeRawBytes(value, 0, value.length);
  }

  public void writeRawBytes(byte[] value, int off, int len) throws IOException {
    if (len > mLimit - mPos) {
      throw new OutOfSpaceException();
    }
    System.arraycopy(value, off, mBuf, mPos, len);
    mPos += len;
  }

  private static void check(int status) throws OutOfSpaceException {
    if (status < 0) {
      throw new OutOfSpaceException();
    }
  }

  // ── position ─────────────────────────────────────────────────────────────

  /** Bytes written since the stream was created. */
  public int getTotalBytesWritten() {
    return mPos - mStart;
  }

  /** Bytes left before the end of the buffer. */
  public int spaceLeft() {
    return mLimit - mPos;
  }

  /** Throws unless the buffer was filled exactly — a message that mis-sized itself. */
  public void checkNoSpaceLeft() {
    if (spaceLeft() != 0) {
      throw new IllegalStateException("Did not write as much data as expected.");
    }
  }

  /** Nothing to flush: the bytes are already in the array. */
  public void flush() {}

  // ── zigzag ───────────────────────────────────────────────────────────────

  public static int encodeZigZag32(int n) {
    return (n << 1) ^ (n >> 31);
  }

  public static long encodeZigZag64(long n) {
    return (n << 1) ^ (n >> 63);
  }

  // ── sizes ────────────────────────────────────────────────────────────────

  public static int computeTagSize(int fieldNumber) {
    return computeUInt32SizeNoTag(WireFormat.makeTag(fieldNumber, 0));
  }

  public static int computeInt32SizeNoTag(int value) {
    return value >= 0 ? computeUInt32SizeNoTag(value) : 10;
  }

  public static int computeUInt32SizeNoTag(int value) {
    int n = 1;
    while ((value & ~0x7F) != 0) {
      n++;
      value >>>= 7;
    }
    return n;
  }

  public static int computeSInt32SizeNoTag(int value) {
    return computeUInt32SizeNoTag(encodeZigZag32(value));
  }

  public static int computeInt64SizeNoTag(long value) {
    return computeUInt64SizeNoTag(value);
  }

  public static int computeUInt64SizeNoTag(long value) {
    int n = 1;
    while ((value & ~0x7FL) != 0L) {
      n++;
      value >>>= 7;
    }
    return n;
  }

  public static int computeSInt64SizeNoTag(long value) {
    return computeUInt64SizeNoTag(encodeZigZag64(value));
  }

  public static int computeFixed32SizeNoTag(int value) {
    return 4;
  }

  public static int computeSFixed32SizeNoTag(int value) {
    return 4;
  }

  public static int computeFixed64SizeNoTag(long value) {
    return 8;
  }

  public static int computeSFixed64SizeNoTag(long value) {
    return 8;
  }

  public static int computeBoolSizeNoTag(boolean value) {
    return 1;
  }

  public static int computeEnumSizeNoTag(int value) {
    return computeInt32SizeNoTag(value);
  }

  public static int computeFloatSizeNoTag(float value) {
    return 4;
  }

  public static int computeDoubleSizeNoTag(double value) {
    return 8;
  }

  public static int computeStringSizeNoTag(String value) {
    int len = value.getBytes().length;
    return computeUInt32SizeNoTag(len) + len;
  }

  public static int computeBytesSizeNoTag(byte[] value) {
    return computeUInt32SizeNoTag(value.length) + value.length;
  }

  public static int computeMessageSizeNoTag(MessageLite value) {
    int size = value.getSerializedSize();
    return computeUInt32SizeNoTag(size) + size;
  }

  public static int computeInt32Size(int fieldNumber, int value) {
    return computeTagSize(fieldNumber) + computeInt32SizeNoTag(value);
  }

  public static int computeUInt32Size(int fieldNumber, int value) {
    return computeTagSize(fieldNumber) + computeUInt32SizeNoTag(value);
  }

  public static int computeSInt32Size(int fieldNumber, int value) {
    return computeTagSize(fieldNumber) + computeSInt32SizeNoTag(value);
  }

  public static int computeFixed32Size(int fieldNumber, int value) {
    return computeTagSize(fieldNumber) + 4;
  }

  public static int computeSFixed32Size(int fieldNumber, int value) {
    return computeTagSize(fieldNumber) + 4;
  }

  public static int computeInt64Size(int fieldNumber, long value) {
    return computeTagSize(fieldNumber) + computeUInt64SizeNoTag(value);
  }

  public static int computeUInt64Size(int fieldNumber, long value) {
    return computeTagSize(fieldNumber) + computeUInt64SizeNoTag(value);
  }

  public static int computeSInt64Size(int fieldNumber, long value) {
    return computeTagSize(fieldNumber) + computeSInt64SizeNoTag(value);
  }

  public static int computeFixed64Size(int fieldNumber, long value) {
    return computeTagSize(fieldNumber) + 8;
  }

  public static int computeSFixed64Size(int fieldNumber, long value) {
    return computeTagSize(fieldNumber) + 8;
  }

  public static int computeBoolSize(int fieldNumber, boolean value) {
    return computeTagSize(fieldNumber) + 1;
  }

  public static int computeEnumSize(int fieldNumber, int value) {
    return computeTagSize(fieldNumber) + computeInt32SizeNoTag(value);
  }

  public static int computeFloatSize(int fieldNumber, float value) {
    return computeTagSize(fieldNumber) + 4;
  }

  public static int computeDoubleSize(int fieldNumber, double value) {
    return computeTagSize(fieldNumber) + 8;
  }

  public static int computeStringSize(int fieldNumber, String value) {
    return computeTagSize(fieldNumber) + computeStringSizeNoTag(value);
  }

  public static int computeBytesSize(int fieldNumber, byte[] value) {
    return computeTagSize(fieldNumber) + computeBytesSizeNoTag(value);
  }

  public static int computeMessageSize(int fieldNumber, MessageLite value) {
    return computeTagSize(fieldNumber) + computeMessageSizeNoTag(value);
  }

  // ── natives (picodroid-core/src/native_handler/protobuf/mod.rs) ──────────
  // Each reads mBuf/mPos/mLimit from the stream, encodes one value with the
  // native codec and stores the new mPos. They answer 0, or -1 when the
  // value did not fit (the bytes that did are written).

  static native int nativeWriteVarint32(Object self, int value);

  static native int nativeWriteVarint64(Object self, long value);

  static native int nativeWriteFixed32(Object self, int value);

  static native int nativeWriteFixed64(Object self, long value);

  static native int nativeWriteDouble(Object self, double value);
}
