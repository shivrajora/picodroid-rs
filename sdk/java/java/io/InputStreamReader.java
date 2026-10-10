// SPDX-License-Identifier: GPL-3.0-only
package java.io;

/**
 * Mirrors {@code java.io.InputStreamReader}: a {@link Reader} over an {@link InputStream}. Chars
 * are the stream's bytes, one each (see {@link Reader}); the charset name of the two-argument
 * constructor is accepted for source compatibility and reported back by {@link #getEncoding()}.
 */
public class InputStreamReader extends Reader {
  private static final int BUF_SIZE = 64;

  private final InputStream in;
  private final String encoding;
  private byte[] buf;
  private int pos;
  private int limit;

  public InputStreamReader(InputStream in) {
    this.in = in;
    this.encoding = "UTF-8";
  }

  public InputStreamReader(InputStream in, String charsetName) throws UnsupportedEncodingException {
    if (charsetName == null) {
      throw new NullPointerException("charsetName");
    }
    this.in = in;
    this.encoding = charsetName;
  }

  public String getEncoding() {
    return encoding;
  }

  /** Refills the byte buffer from the stream; false at end of stream. */
  private boolean fill() throws IOException {
    if (buf == null) {
      buf = new byte[BUF_SIZE];
    }
    int n = in.read(buf, 0, buf.length);
    if (n <= 0) {
      pos = 0;
      limit = 0;
      return false;
    }
    pos = 0;
    limit = n;
    return true;
  }

  @Override
  public int read() throws IOException {
    if (pos >= limit && !fill()) {
      return -1;
    }
    return buf[pos++] & 0xff;
  }

  @Override
  public int read(char[] cbuf, int off, int len) throws IOException {
    if (cbuf == null) {
      throw new NullPointerException();
    }
    if (off < 0 || len < 0 || len > cbuf.length - off) {
      throw new IndexOutOfBoundsException();
    }
    if (len == 0) {
      return 0;
    }
    if (pos >= limit && !fill()) {
      return -1;
    }
    int n = Math.min(len, limit - pos);
    for (int i = 0; i < n; i++) {
      cbuf[off + i] = (char) (buf[pos + i] & 0xff);
    }
    pos += n;
    return n;
  }

  @Override
  public boolean ready() throws IOException {
    return pos < limit || in.available() > 0;
  }

  @Override
  public void close() throws IOException {
    in.close();
  }
}
