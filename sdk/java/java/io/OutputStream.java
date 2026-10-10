// SPDX-License-Identifier: GPL-3.0-only
package java.io;

/**
 * Mirrors {@code java.io.OutputStream}: the abstract byte sink every picodroid output stream
 * extends ({@code picodroid.io.FileOutputStream}, {@code picodroid.net.HttpOutputStream}, {@code
 * Socket.getOutputStream()}, {@link ByteArrayOutputStream}). A subclass implements {@link
 * #write(int)} and, for speed, overrides {@link #write(byte[], int, int)}.
 *
 * <p>Declares no fields: the native file and HTTP streams address their own fields by slot.
 */
public abstract class OutputStream implements Closeable {
  /**
   * Writes the low byte of {@code b}.
   *
   * @throws IOException if the byte cannot be stored or sent
   */
  public abstract void write(int b) throws IOException;

  public void write(byte[] b) throws IOException {
    write(b, 0, b.length);
  }

  /** The JDK's default: one {@link #write(int)} per byte. */
  public void write(byte[] b, int off, int len) throws IOException {
    if (b == null) {
      throw new NullPointerException();
    }
    if (off < 0 || len < 0 || len > b.length - off) {
      throw new IndexOutOfBoundsException();
    }
    for (int i = 0; i < len; i++) {
      write(b[off + i]);
    }
  }

  public void flush() throws IOException {}

  @Override
  public void close() throws IOException {}
}
