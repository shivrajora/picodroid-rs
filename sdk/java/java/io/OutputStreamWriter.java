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
    encodeChar((char) c);
    out.write(chunk, 0, chunkLen);
    chunkLen = 0;
  }

  private int chunkLen;

  /**
   * Appends {@code c} to {@code chunk} as UTF-8 (the default charset, and what every charset name
   * here encodes): one byte for ASCII, two or three for the rest of the BMP; a lone surrogate has
   * no encoding and becomes {@code ?}. {@link #write(String)} sends a String's bytes, which are
   * UTF-8 already, as they are.
   */
  private void encodeChar(char c) {
    if (chunk == null) {
      chunk = new byte[CHUNK];
    }
    if (c < 0x80) {
      chunk[chunkLen++] = (byte) c;
    } else if (c < 0x800) {
      chunk[chunkLen++] = (byte) (0xC0 | (c >> 6));
      chunk[chunkLen++] = (byte) (0x80 | (c & 0x3F));
    } else if (c >= 0xD800 && c <= 0xDFFF) {
      chunk[chunkLen++] = (byte) '?';
    } else {
      chunk[chunkLen++] = (byte) (0xE0 | (c >> 12));
      chunk[chunkLen++] = (byte) (0x80 | ((c >> 6) & 0x3F));
      chunk[chunkLen++] = (byte) (0x80 | (c & 0x3F));
    }
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
    for (int i = 0; i < len; i++) {
      if (chunkLen > chunk.length - 3) {
        out.write(chunk, 0, chunkLen);
        chunkLen = 0;
      }
      encodeChar(cbuf[off + i]);
    }
    if (chunkLen > 0) {
      out.write(chunk, 0, chunkLen);
      chunkLen = 0;
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
