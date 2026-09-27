// SPDX-License-Identifier: GPL-3.0-only
package picodroid.protobuf;

import java.io.IOException;

/**
 * Reads protobuf wire-format values out of a {@code byte[]}, with the method names and semantics of
 * {@code com.google.protobuf.CodedInputStream}: {@link #readTag} then one {@code readXxx} per
 * field, {@link #skipField} for a field the reader does not know, and {@link #pushLimit}/{@link
 * #popLimit} around an embedded message.
 *
 * <p>Available only on boards whose board.toml sets {@code has_protobuf = true}. The byte-at-a-time
 * work (tags, varints, fixed-width values, skipping) is native; slicing a string or a {@code
 * byte[]} out of the buffer is Java. Deviations from javalite: no {@code ByteString} ({@link
 * #readBytes} returns a {@code byte[]}), no {@code InputStream} sources (this runtime has none), no
 * extension registries, no groups (a group tag throws), and {@link #readString} does not validate
 * UTF-8 (the runtime's strings are byte strings).
 */
public final class CodedInputStream {
  // Slot-addressed from Rust (native_handler/protobuf/fields.rs): these instance fields stay in
  // this order, with nothing declared before them.
  private final byte[] mBuf;
  private int mPos;
  private int mLimit;
  private int mLastTag;
  private final int mStart;

  private CodedInputStream(byte[] buf, int off, int len) {
    mBuf = buf;
    mPos = off;
    mLimit = off + len;
    mStart = off;
  }

  /** A stream over all of {@code buf}. */
  public static CodedInputStream newInstance(byte[] buf) {
    return newInstance(buf, 0, buf.length);
  }

  /** A stream over {@code len} bytes of {@code buf} starting at {@code off}. */
  public static CodedInputStream newInstance(byte[] buf, int off, int len) {
    if (buf == null) {
      throw new NullPointerException("buf == null");
    }
    if (off < 0 || len < 0 || off + len > buf.length) {
      throw new ArrayIndexOutOfBoundsException(off + len);
    }
    return new CodedInputStream(buf, off, len);
  }

  // ── tags ─────────────────────────────────────────────────────────────────

  /**
   * The next field's tag, or {@code 0} at the end of the stream (or of the current limit). A tag
   * with field number zero throws.
   */
  public int readTag() throws IOException {
    if (isAtEnd()) {
      mLastTag = 0;
      return 0;
    }
    int tag = nativeReadTag(this);
    if (WireFormat.getTagFieldNumber(tag) == 0) {
      // A zero field number is never valid; javalite throws here too rather than
      // letting a stray zero byte read as the end of the message.
      mLastTag = 0;
      throw InvalidProtocolBufferException.invalidTag();
    }
    mLastTag = tag;
    return tag;
  }

  /** The tag {@link #readTag} last returned. */
  public int getLastTag() {
    return mLastTag;
  }

  /** Throws unless the last tag read was {@code value}. */
  public void checkLastTagWas(int value) throws InvalidProtocolBufferException {
    if (mLastTag != value) {
      throw InvalidProtocolBufferException.invalidEndTag();
    }
  }

  /**
   * Skips the value of the field whose tag is {@code tag}. Returns {@code true}; kept as a boolean
   * for javalite's signature, where only an end-group tag yields {@code false} and groups are not
   * supported here.
   */
  public boolean skipField(int tag) throws IOException {
    nativeSkipField(this, tag);
    return true;
  }

  /** Skips every remaining field up to the end of the stream or the current limit. */
  public void skipMessage() throws IOException {
    while (true) {
      int tag = readTag();
      if (tag == 0) {
        return;
      }
      skipField(tag);
    }
  }

  // ── values ───────────────────────────────────────────────────────────────

  public int readInt32() throws IOException {
    return nativeReadVarint32(this);
  }

  public int readUInt32() throws IOException {
    return nativeReadVarint32(this);
  }

  public int readSInt32() throws IOException {
    return decodeZigZag32(nativeReadVarint32(this));
  }

  public int readEnum() throws IOException {
    return nativeReadVarint32(this);
  }

  public long readInt64() throws IOException {
    return nativeReadVarint64(this);
  }

  public long readUInt64() throws IOException {
    return nativeReadVarint64(this);
  }

  public long readSInt64() throws IOException {
    return decodeZigZag64(nativeReadVarint64(this));
  }

  public boolean readBool() throws IOException {
    return nativeReadVarint64(this) != 0L;
  }

  public int readFixed32() throws IOException {
    return nativeReadFixed32(this);
  }

  public int readSFixed32() throws IOException {
    return nativeReadFixed32(this);
  }

  public long readFixed64() throws IOException {
    return nativeReadFixed64(this);
  }

  public long readSFixed64() throws IOException {
    return nativeReadFixed64(this);
  }

  public float readFloat() throws IOException {
    return Float.intBitsToFloat(nativeReadFixed32(this));
  }

  public double readDouble() throws IOException {
    return nativeReadDouble(this);
  }

  /** A length-prefixed string, taken as the runtime's byte string without UTF-8 validation. */
  public String readString() throws IOException {
    int len = readLength();
    String s = new String(mBuf, mPos, len);
    mPos += len;
    return s;
  }

  /** A length-prefixed {@code byte[]} (javalite returns a {@code ByteString} here). */
  public byte[] readBytes() throws IOException {
    return readRawBytes(readLength());
  }

  /** The raw varint under the cursor, low 32 bits. */
  public int readRawVarint32() throws IOException {
    return nativeReadVarint32(this);
  }

  /** The raw varint under the cursor. */
  public long readRawVarint64() throws IOException {
    return nativeReadVarint64(this);
  }

  /** One byte, as a {@code byte}. */
  public byte readRawByte() throws IOException {
    if (mPos == mLimit) {
      throw InvalidProtocolBufferException.truncatedMessage();
    }
    return mBuf[mPos++];
  }

  /** {@code size} bytes, copied out. */
  public byte[] readRawBytes(int size) throws IOException {
    if (size < 0) {
      throw InvalidProtocolBufferException.negativeSize();
    }
    if (size > mLimit - mPos) {
      throw InvalidProtocolBufferException.truncatedMessage();
    }
    byte[] out = new byte[size];
    System.arraycopy(mBuf, mPos, out, 0, size);
    mPos += size;
    return out;
  }

  /** A length prefix, checked against the remaining bytes. */
  private int readLength() throws IOException {
    int len = nativeReadVarint32(this);
    if (len < 0) {
      throw InvalidProtocolBufferException.negativeSize();
    }
    if (len > mLimit - mPos) {
      throw InvalidProtocolBufferException.truncatedMessage();
    }
    return len;
  }

  // ── limits ───────────────────────────────────────────────────────────────

  /**
   * Narrows the stream to the next {@code byteLimit} bytes — an embedded message — and returns the
   * value to hand back to {@link #popLimit}.
   */
  public int pushLimit(int byteLimit) throws InvalidProtocolBufferException {
    if (byteLimit < 0) {
      throw InvalidProtocolBufferException.negativeSize();
    }
    int newLimit = mPos + byteLimit;
    if (newLimit > mLimit) {
      throw InvalidProtocolBufferException.truncatedMessage();
    }
    int old = mLimit;
    mLimit = newLimit;
    return old;
  }

  /** Restores the limit {@link #pushLimit} replaced. */
  public void popLimit(int oldLimit) {
    mLimit = oldLimit;
  }

  /** Bytes left before the current limit. */
  public int getBytesUntilLimit() {
    return mLimit - mPos;
  }

  /** Whether the cursor is at the end of the stream or the current limit. */
  public boolean isAtEnd() {
    return mPos == mLimit;
  }

  /** Bytes consumed since the stream was created. */
  public int getTotalBytesRead() {
    return mPos - mStart;
  }

  // ── zigzag ───────────────────────────────────────────────────────────────

  public static int decodeZigZag32(int n) {
    return (n >>> 1) ^ -(n & 1);
  }

  public static long decodeZigZag64(long n) {
    return (n >>> 1) ^ -(n & 1);
  }

  // ── natives (picodroid-core/src/native_handler/protobuf/mod.rs) ──────────
  // Each reads mBuf/mPos/mLimit from the stream, decodes one value with the
  // native codec, stores the new mPos, and throws
  // InvalidProtocolBufferException on truncated or malformed input.

  static native int nativeReadTag(Object self) throws IOException;

  static native int nativeReadVarint32(Object self) throws IOException;

  static native long nativeReadVarint64(Object self) throws IOException;

  static native int nativeReadFixed32(Object self) throws IOException;

  static native long nativeReadFixed64(Object self) throws IOException;

  static native double nativeReadDouble(Object self) throws IOException;

  static native void nativeSkipField(Object self, int tag) throws IOException;
}
