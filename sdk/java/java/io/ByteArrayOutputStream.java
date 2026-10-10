// SPDX-License-Identifier: GPL-3.0-only
package java.io;

/**
 * Mirrors {@code java.io.ByteArrayOutputStream}: an {@link OutputStream} that grows a byte array.
 * {@link #toString()} builds a {@code String} from the bytes the way {@code new String(byte[])}
 * does here: one char per byte, bytes above 0x7F as {@code ?}.
 */
public class ByteArrayOutputStream extends OutputStream {
  protected byte[] buf;
  protected int count;

  public ByteArrayOutputStream() {
    this(32);
  }

  public ByteArrayOutputStream(int size) {
    if (size < 0) {
      throw new IllegalArgumentException("Negative initial size: " + size);
    }
    buf = new byte[size];
  }

  private void ensureCapacity(int minCapacity) {
    if (minCapacity > buf.length) {
      int newCapacity = buf.length * 2;
      if (newCapacity < minCapacity) {
        newCapacity = minCapacity;
      }
      byte[] bigger = new byte[newCapacity];
      System.arraycopy(buf, 0, bigger, 0, count);
      buf = bigger;
    }
  }

  @Override
  public void write(int b) {
    ensureCapacity(count + 1);
    buf[count] = (byte) b;
    count += 1;
  }

  @Override
  public void write(byte[] b, int off, int len) {
    if (b == null) {
      throw new NullPointerException();
    }
    if (off < 0 || len < 0 || len > b.length - off) {
      throw new IndexOutOfBoundsException();
    }
    ensureCapacity(count + len);
    System.arraycopy(b, off, buf, count, len);
    count += len;
  }

  /** Copies everything written so far into {@code out}. */
  public void writeTo(OutputStream out) throws IOException {
    out.write(buf, 0, count);
  }

  public void reset() {
    count = 0;
  }

  public byte[] toByteArray() {
    byte[] copy = new byte[count];
    System.arraycopy(buf, 0, copy, 0, count);
    return copy;
  }

  public int size() {
    return count;
  }

  @Override
  public String toString() {
    return new String(buf, 0, count);
  }

  /** The charset name is accepted for source compatibility; strings here are byte-backed. */
  public String toString(String charsetName) {
    return new String(buf, 0, count);
  }

  @Override
  public void close() {}
}
