// SPDX-License-Identifier: GPL-3.0-only
package java.io;

/**
 * Mirrors {@code java.io.Reader}: the abstract character source behind {@link InputStreamReader}
 * and {@link BufferedReader}. A subclass implements {@link #read(char[], int, int)} and {@link
 * #close()}.
 *
 * <p>Strings here are byte-backed (one {@code char} per byte, see {@code String.charAt}), so a
 * reader hands bytes up as chars unchanged: UTF-8 text survives a round trip through {@code
 * StringBuilder.append(char)}, and a byte above 0x7F is one char, not a decoded code point.
 */
public abstract class Reader implements Closeable {
  private static final int SKIP_CHUNK = 64;

  protected Reader() {}

  /** The next char as 0..255, or -1 at end of stream. */
  public int read() throws IOException {
    char[] one = new char[1];
    int n = read(one, 0, 1);
    return n <= 0 ? -1 : one[0];
  }

  public int read(char[] cbuf) throws IOException {
    return read(cbuf, 0, cbuf.length);
  }

  /** Reads up to {@code len} chars into {@code cbuf} at {@code off}; -1 at end of stream. */
  public abstract int read(char[] cbuf, int off, int len) throws IOException;

  public long skip(long n) throws IOException {
    if (n < 0) {
      throw new IllegalArgumentException("skip value is negative");
    }
    long remaining = n;
    char[] buf = new char[(int) Math.min(SKIP_CHUNK, remaining)];
    while (remaining > 0) {
      int got = read(buf, 0, (int) Math.min(buf.length, remaining));
      if (got < 0) {
        break;
      }
      remaining -= got;
    }
    return n - remaining;
  }

  /** Whether the next {@link #read()} is known not to block; {@code false} unless overridden. */
  public boolean ready() throws IOException {
    return false;
  }

  public boolean markSupported() {
    return false;
  }

  public void mark(int readAheadLimit) throws IOException {
    throw new IOException("mark() not supported");
  }

  public void reset() throws IOException {
    throw new IOException("reset() not supported");
  }

  @Override
  public abstract void close() throws IOException;
}
