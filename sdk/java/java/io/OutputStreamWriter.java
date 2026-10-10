// SPDX-License-Identifier: GPL-3.0-only
package java.io;

/**
 * Mirrors {@code java.io.OutputStreamWriter}: a {@link Writer} over an {@link OutputStream}. Each
 * char goes out as one byte (see {@link Reader}); a {@code String} is written as its bytes in one
 * call. The charset name is accepted for source compatibility and reported by {@link
 * #getEncoding()}.
 */
public class OutputStreamWriter extends Writer {
  private static final int CHUNK = 64;

  private final OutputStream out;
  private final String encoding;
  private byte[] chunk;

  public OutputStreamWriter(OutputStream out) {
    this.out = out;
    this.encoding = "UTF-8";
  }

  public OutputStreamWriter(OutputStream out, String charsetName)
      throws UnsupportedEncodingException {
    if (charsetName == null) {
      throw new NullPointerException("charsetName");
    }
    this.out = out;
    this.encoding = charsetName;
  }

  public String getEncoding() {
    return encoding;
  }

  @Override
  public void write(int c) throws IOException {
    out.write(c);
  }

  @Override
  public void write(char[] cbuf, int off, int len) throws IOException {
    if (cbuf == null) {
      throw new NullPointerException();
    }
    if (off < 0 || len < 0 || len > cbuf.length - off) {
      throw new IndexOutOfBoundsException();
    }
    if (chunk == null) {
      chunk = new byte[CHUNK];
    }
    while (len > 0) {
      int n = Math.min(len, chunk.length);
      for (int i = 0; i < n; i++) {
        chunk[i] = (byte) cbuf[off + i];
      }
      out.write(chunk, 0, n);
      off += n;
      len -= n;
    }
  }

  @Override
  public void write(String str) throws IOException {
    byte[] bytes = str.getBytes();
    out.write(bytes, 0, bytes.length);
  }

  @Override
  public void flush() throws IOException {
    out.flush();
  }

  @Override
  public void close() throws IOException {
    out.flush();
    out.close();
  }
}
