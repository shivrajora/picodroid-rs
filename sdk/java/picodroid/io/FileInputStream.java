// SPDX-License-Identifier: GPL-3.0-only
package picodroid.io;

import java.io.InputStream;

/**
 * Reads a file in this app's private storage. A {@link java.io.InputStream}, so it wraps in an
 * {@code InputStreamReader} / {@code BufferedReader} as on Android. Each {@code read} is a
 * standalone native call; there is no native handle to release.
 *
 * <p>Field order is addressed by slot from native code ({@code native_handler/io}): keep {@code
 * path} first and {@code pos} second.
 */
public class FileInputStream extends InputStream {
  private String path;
  private long pos;

  public FileInputStream(File f) {
    this.path = f.getPath();
    this.pos = 0;
  }

  public FileInputStream(String path) {
    this.path = path;
    this.pos = 0;
  }

  /** Reads up to {@code len} bytes; the count read, or -1 at end of file. */
  @Override
  public native int read(byte[] buf, int off, int len);

  @Override
  public int read(byte[] buf) {
    return read(buf, 0, buf.length);
  }

  /** The next byte as 0..255, or -1 at end of file — one native read of one byte. */
  @Override
  public int read() {
    byte[] one = new byte[1];
    int n = read(one, 0, 1);
    return n <= 0 ? -1 : (one[0] & 0xff);
  }

  @Override
  public native int available();

  @Override
  public void close() {
    // No native handle to release — each read() is standalone.
  }
}
