// SPDX-License-Identifier: GPL-3.0-only
package picodroid.net;

import java.io.IOException;
import java.io.InputStream;

/**
 * Reads the response body of an {@link HttpURLConnection}. A {@link java.io.InputStream}, so it
 * wraps in an {@code InputStreamReader} / {@code BufferedReader} as on Android. Close the parent
 * connection to free.
 *
 * <p>{@code handle} is addressed by slot from native code ({@code net/fields.rs}): keep it first.
 */
public class HttpInputStream extends InputStream {
  private int handle;

  HttpInputStream(int handle) {
    this.handle = handle;
  }

  /** The next byte as 0..255, or -1 at end of body — one native read of one byte. */
  @Override
  public int read() throws IOException {
    byte[] one = new byte[1];
    int n = read(one, 0, 1);
    return n <= 0 ? -1 : (one[0] & 0xff);
  }

  /**
   * Read up to {@code len} bytes of the response body.
   *
   * @return bytes read, or -1 at orderly end of stream — never -1 for errors
   * @throws java.net.SocketTimeoutException if a read timeout expired (a stalled server no longer
   *     reads as end-of-stream)
   * @throws IOException for any other receive failure
   */
  @Override
  public native int read(byte[] buf, int off, int len) throws IOException;

  @Override
  public int read(byte[] buf) throws IOException {
    return read(buf, 0, buf.length);
  }

  @Override
  public void close() {
    // Resource is owned by the parent HttpURLConnection.
  }
}
