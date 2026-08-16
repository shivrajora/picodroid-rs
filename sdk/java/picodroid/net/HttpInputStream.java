// SPDX-License-Identifier: GPL-3.0-only
package picodroid.net;

import java.io.IOException;

/** Reads the response body of an {@link HttpURLConnection}. Close the parent connection to free. */
public class HttpInputStream implements AutoCloseable {
  private int handle;

  HttpInputStream(int handle) {
    this.handle = handle;
  }

  /**
   * Read up to {@code len} bytes of the response body.
   *
   * @return bytes read, or -1 at orderly end of stream — never -1 for errors
   * @throws java.net.SocketTimeoutException if a read timeout expired (a stalled server no longer
   *     reads as end-of-stream)
   * @throws IOException for any other receive failure
   */
  public native int read(byte[] buf, int off, int len) throws IOException;

  public int read(byte[] buf) throws IOException {
    return read(buf, 0, buf.length);
  }

  @Override
  public void close() {
    // Resource is owned by the parent HttpURLConnection.
  }
}
