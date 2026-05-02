// SPDX-License-Identifier: GPL-3.0-only
package picodroid.net;

/** Writes the request body of an {@link HttpUrlConnection}. Close the parent connection to free. */
public class HttpOutputStream implements AutoCloseable {
  private int handle;

  HttpOutputStream(int handle) {
    this.handle = handle;
  }

  public native void write(byte[] buf, int off, int len);

  public void write(byte[] buf) {
    write(buf, 0, buf.length);
  }

  public void write(int b) {
    byte[] one = new byte[1];
    one[0] = (byte) b;
    write(one, 0, 1);
  }

  @Override
  public void close() {
    // Resource is owned by the parent HttpUrlConnection.
  }
}
