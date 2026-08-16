// SPDX-License-Identifier: GPL-3.0-only
package picodroid.net;

import java.io.IOException;

/** Writes the request body of an {@link HttpURLConnection}. Close the parent connection to free. */
public class HttpOutputStream implements AutoCloseable {
  private int handle;

  HttpOutputStream(int handle) {
    this.handle = handle;
  }

  /**
   * Write {@code len} bytes of the request body.
   *
   * @throws java.net.SocketException if the connection was reset or closed
   * @throws IOException for any other send failure
   */
  public native void write(byte[] buf, int off, int len) throws IOException;

  public void write(byte[] buf) throws IOException {
    write(buf, 0, buf.length);
  }

  public void write(int b) throws IOException {
    byte[] one = new byte[1];
    one[0] = (byte) b;
    write(one, 0, 1);
  }

  @Override
  public void close() {
    // Resource is owned by the parent HttpURLConnection.
  }
}
