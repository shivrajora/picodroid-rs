// SPDX-License-Identifier: GPL-3.0-only
package picodroid.io;

public class FileInputStream implements AutoCloseable {
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

  public native int read(byte[] buf, int off, int len);

  public int read(byte[] buf) {
    return read(buf, 0, buf.length);
  }

  public native int available();

  @Override
  public void close() {
    // No native handle to release — each read() is standalone.
  }
}
