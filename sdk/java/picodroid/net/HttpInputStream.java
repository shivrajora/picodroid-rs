// SPDX-License-Identifier: GPL-3.0-only
package picodroid.net;

/** Reads the response body of an {@link HttpURLConnection}. Close the parent connection to free. */
public class HttpInputStream implements AutoCloseable {
  private int handle;

  HttpInputStream(int handle) {
    this.handle = handle;
  }

  public native int read(byte[] buf, int off, int len);

  public int read(byte[] buf) {
    return read(buf, 0, buf.length);
  }

  @Override
  public void close() {
    // Resource is owned by the parent HttpURLConnection.
  }
}
