// SPDX-License-Identifier: GPL-3.0-only
package java.io;

/**
 * Mirrors {@code java.io.BufferedReader}: buffers a {@link Reader} and adds {@link #readLine()}.
 * The usual shape works unchanged:
 *
 * <pre>{@code
 * BufferedReader r = new BufferedReader(new InputStreamReader(conn.getInputStream()));
 * String line;
 * while ((line = r.readLine()) != null) { ... }
 * }</pre>
 *
 * A line is built one {@code StringBuilder.append(char)} at a time, so every byte of a UTF-8 line
 * reaches the string unchanged (see {@link Reader}). The default buffer is 128 chars; pass a size
 * to the two-argument constructor for larger reads.
 */
public class BufferedReader extends Reader {
  private static final int DEFAULT_BUFFER_SIZE = 128;

  private final Reader in;
  private char[] cb;
  private int nextChar;
  private int nChars;

  /** A line ended in {@code \r}: swallow the {@code \n} that may follow it. */
  private boolean skipLf;

  public BufferedReader(Reader in) {
    this(in, DEFAULT_BUFFER_SIZE);
  }

  public BufferedReader(Reader in, int size) {
    if (size <= 0) {
      throw new IllegalArgumentException("Buffer size <= 0");
    }
    this.in = in;
    this.cb = new char[size];
  }

  private void ensureOpen() throws IOException {
    if (cb == null) {
      throw new IOException("Stream closed");
    }
  }

  /** Refills the buffer; false at end of stream. */
  private boolean fill() throws IOException {
    int n = in.read(cb, 0, cb.length);
    if (n <= 0) {
      nextChar = 0;
      nChars = 0;
      return false;
    }
    nextChar = 0;
    nChars = n;
    return true;
  }

  @Override
  public int read() throws IOException {
    ensureOpen();
    while (true) {
      if (nextChar >= nChars && !fill()) {
        return -1;
      }
      char c = cb[nextChar++];
      if (skipLf) {
        skipLf = false;
        if (c == '\n') {
          continue;
        }
      }
      return c;
    }
  }

  @Override
  public int read(char[] cbuf, int off, int len) throws IOException {
    ensureOpen();
    if (cbuf == null) {
      throw new NullPointerException();
    }
    if (off < 0 || len < 0 || len > cbuf.length - off) {
      throw new IndexOutOfBoundsException();
    }
    if (len == 0) {
      return 0;
    }
    if (skipLf) {
      // Resolve the pending \r\n before a bulk copy, one char at a time.
      int c = read();
      if (c == -1) {
        return -1;
      }
      cbuf[off] = (char) c;
      return 1;
    }
    if (nextChar >= nChars && !fill()) {
      return -1;
    }
    int n = Math.min(len, nChars - nextChar);
    System.arraycopy(cb, nextChar, cbuf, off, n);
    nextChar += n;
    return n;
  }

  /**
   * The next line without its terminator ({@code \n}, {@code \r} or {@code \r\n}), or {@code null}
   * when the stream ended before any char of a line was read. A final unterminated line is returned
   * as it is.
   */
  public String readLine() throws IOException {
    ensureOpen();
    StringBuilder sb = null;
    while (true) {
      if (nextChar >= nChars && !fill()) {
        return sb == null ? null : sb.toString();
      }
      char c = cb[nextChar++];
      if (skipLf) {
        skipLf = false;
        if (c == '\n') {
          continue;
        }
      }
      if (c == '\n') {
        return sb == null ? "" : sb.toString();
      }
      if (c == '\r') {
        skipLf = true;
        return sb == null ? "" : sb.toString();
      }
      if (sb == null) {
        sb = new StringBuilder();
      }
      sb.append(c);
    }
  }

  @Override
  public long skip(long n) throws IOException {
    ensureOpen();
    return super.skip(n);
  }

  @Override
  public boolean ready() throws IOException {
    ensureOpen();
    return nextChar < nChars || in.ready();
  }

  @Override
  public void close() throws IOException {
    if (cb == null) {
      return;
    }
    cb = null;
    in.close();
  }
}
