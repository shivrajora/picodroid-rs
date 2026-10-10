// SPDX-License-Identifier: GPL-3.0-only
package picodroid.io;

import java.io.IOException;
import java.io.OutputStream;

/**
 * Writes to a file in this app's private storage. A {@link java.io.OutputStream}, so it wraps in an
 * {@code OutputStreamWriter} / {@code PrintWriter} as on Android. Each {@code write} is a
 * standalone native call that throws {@link IOException} when the bytes cannot be stored — the
 * volume is full, the app is over its storage cap, or the path was refused.
 *
 * <p>Field order is addressed by slot from native code ({@code native_handler/io}): keep {@code
 * path} first and {@code pos} second.
 */
public class FileOutputStream extends OutputStream {
  private String path;
  private long pos;

  public FileOutputStream(File f) {
    this(f.getPath(), false);
  }

  public FileOutputStream(String path) {
    this(path, false);
  }

  public FileOutputStream(String path, boolean append) {
    this.path = path;
    this.pos = initStream(path, append);
  }

  // Truncates when append=false, returns current file size when append=true.
  private static native long initStream(String path, boolean append);

  @Override
  public native void write(byte[] buf, int off, int len) throws IOException;

  @Override
  public void write(byte[] buf) throws IOException {
    write(buf, 0, buf.length);
  }

  @Override
  public void write(int b) throws IOException {
    byte[] one = new byte[1];
    one[0] = (byte) b;
    write(one, 0, 1);
  }

  @Override
  public native void flush();

  @Override
  public void close() {
    // No native handle to release — each write() is standalone.
  }
}
