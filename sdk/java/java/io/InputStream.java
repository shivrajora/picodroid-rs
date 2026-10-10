// SPDX-License-Identifier: GPL-3.0-only
package java.io;

/**
 * Mirrors {@code java.io.InputStream}: the abstract byte source every picodroid input stream
 * extends ({@code picodroid.io.FileInputStream}, {@code picodroid.net.HttpInputStream}, {@code
 * Socket.getInputStream()}, {@link ByteArrayInputStream}). A subclass implements {@link #read()}
 * and, for speed, overrides {@link #read(byte[], int, int)}; the base class supplies the rest of
 * the JDK surface on top of those two.
 *
 * <p>Declares no fields: the native file and HTTP streams address their own fields by slot, and a
 * field here would shift every one of them.
 */
public abstract class InputStream implements Closeable {
  /** Largest single skip buffer: a skip of any length works through this. */
  private static final int SKIP_CHUNK = 64;

  /**
   * The next byte as 0..255, or -1 at end of stream.
   *
   * @throws IOException if the byte cannot be read
   */
  public abstract int read() throws IOException;

  public int read(byte[] b) throws IOException {
    return read(b, 0, b.length);
  }

  /**
   * Reads up to {@code len} bytes into {@code b} at {@code off}; blocks until at least one is
   * available. Returns the count, or -1 at end of stream. The JDK's default, one {@link #read()}
   * per byte, with the first byte's exception propagated and later ones ending the call short.
   */
  public int read(byte[] b, int off, int len) throws IOException {
    if (b == null) {
      throw new NullPointerException();
    }
    if (off < 0 || len < 0 || len > b.length - off) {
      throw new IndexOutOfBoundsException();
    }
    if (len == 0) {
      return 0;
    }
    int c = read();
    if (c == -1) {
      return -1;
    }
    b[off] = (byte) c;
    int i = 1;
    try {
      for (; i < len; i++) {
        c = read();
        if (c == -1) {
          break;
        }
        b[off + i] = (byte) c;
      }
    } catch (IOException e) {
      // The JDK contract: bytes already read are returned, the failure surfaces next call.
    }
    return i;
  }

  /** Skips up to {@code n} bytes by reading them; returns how many were skipped. */
  public long skip(long n) throws IOException {
    long remaining = n;
    if (remaining <= 0) {
      return 0;
    }
    byte[] buf = new byte[(int) Math.min(SKIP_CHUNK, remaining)];
    while (remaining > 0) {
      int got = read(buf, 0, (int) Math.min(buf.length, remaining));
      if (got < 0) {
        break;
      }
      remaining -= got;
    }
    return n - remaining;
  }

  /** Bytes readable without blocking; 0 unless a subclass knows better. */
  public int available() throws IOException {
    return 0;
  }

  @Override
  public void close() throws IOException {}

  /** Mark/reset is opt-in for subclasses, as in the JDK: the base supports neither. */
  public void mark(int readlimit) {}

  public void reset() throws IOException {
    throw new IOException("mark/reset not supported");
  }

  public boolean markSupported() {
    return false;
  }
}
