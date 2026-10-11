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

  @Override
  public int read() throws IOException {
    return decodeChar();
  }

  /**
   * The next char, decoded from UTF-8 (the default charset, and what every charset name here
   * decodes): one byte for ASCII, two or three for the rest of the BMP. A malformed byte or a code
   * point past the BMP comes out as U+FFFD, as the JDK's decoder replaces them. -1 at the end of
   * the stream. A String built from these chars holds the same UTF-8 bytes the stream carried.
   */
  private int decodeChar() throws IOException {
    if (!ensureBytes(1)) {
      return -1;
    }
    int b0 = buf[pos] & 0xff;
    if (b0 < 0x80) {
      pos++;
      return b0;
    }
    int need;
    int cp;
    if (b0 >= 0xC2 && b0 <= 0xDF) {
      need = 1;
      cp = b0 & 0x1F;
    } else if (b0 >= 0xE0 && b0 <= 0xEF) {
      need = 2;
      cp = b0 & 0x0F;
    } else if (b0 >= 0xF0 && b0 <= 0xF4) {
      need = 3;
      cp = b0 & 0x07;
    } else {
      pos++;
      return 0xFFFD; // a stray continuation or an invalid lead byte
    }
    if (!ensureBytes(1 + need)) {
      pos = limit; // a truncated sequence at the end of the stream
      return 0xFFFD;
    }
    for (int i = 1; i <= need; i++) {
      int b = buf[pos + i] & 0xff;
      if ((b & 0xC0) != 0x80) {
        pos++; // the lead byte alone is malformed; the rest is decoded afresh
        return 0xFFFD;
      }
      cp = (cp << 6) | (b & 0x3F);
    }
    pos += 1 + need;
    return cp > 0xFFFF ? 0xFFFD : cp; // chars hold the BMP
  }

  /** Makes {@code n} undecoded bytes available at {@code pos}, reading more as needed. */
  private boolean ensureBytes(int n) throws IOException {
    if (buf == null) {
      buf = new byte[BUF_SIZE];
    }
    while (limit - pos < n) {
      if (pos > 0) {
        System.arraycopy(buf, pos, buf, 0, limit - pos);
        limit -= pos;
        pos = 0;
      }
      int got;
      do {
        // A read that answers 0 (nothing yet, not the end) is tried again, as the JDK does.
        got = in.read(buf, limit, buf.length - limit);
      } while (got == 0);
      if (got < 0) {
        return false;
      }
      limit += got;
    }
    return true;
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
    int n = 0;
    // Decode what is buffered (plus whatever a sequence straddling the buffer end needs); stop
    // before blocking on the stream again once something has been read.
    while (n < len && (n == 0 || pos < limit)) {
      int c = decodeChar();
      if (c < 0) {
        break;
      }
      cbuf[off + n++] = (char) c;
    }
    return n == 0 ? -1 : n;
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
