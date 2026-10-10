// SPDX-License-Identifier: GPL-3.0-only
package picodroid.net;

import java.io.IOException;
import java.io.OutputStream;

/**
 * Writes the request body of an {@link HttpURLConnection}. A {@link java.io.OutputStream}, so it
 * wraps in an {@code OutputStreamWriter} / {@code PrintWriter} as on Android. Close the parent
 * connection to free.
 *
 * <p>{@code handle} is addressed by slot from native code ({@code net/fields.rs}): keep it first.
 */
public class HttpOutputStream extends OutputStream {
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
  public void close() {
    // Resource is owned by the parent HttpURLConnection.
  }
}
