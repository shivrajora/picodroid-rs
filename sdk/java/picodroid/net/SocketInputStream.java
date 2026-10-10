// SPDX-License-Identifier: GPL-3.0-only
package picodroid.net;

import java.io.IOException;
import java.io.InputStream;

/**
 * {@link Socket#getInputStream()}: an {@link InputStream} over {@link Socket#recv}. A read blocks
 * for the socket's timeout and throws {@code SocketTimeoutException} when it expires; -1 is only
 * ever the peer's orderly close. Closing the stream closes the socket, as on Android.
 */
final class SocketInputStream extends InputStream {
  private final Socket socket;

  SocketInputStream(Socket socket) {
    this.socket = socket;
  }

  @Override
  public int read() throws IOException {
    byte[] one = new byte[1];
    int n = socket.recv(one, 0, 1);
    return n <= 0 ? -1 : (one[0] & 0xff);
  }

  @Override
  public int read(byte[] b, int off, int len) throws IOException {
    if (b == null) {
      throw new NullPointerException();
    }
    if (off < 0 || len < 0 || len > b.length - off) {
      throw new IndexOutOfBoundsException();
    }
    if (len == 0) {
      return 0;
    }
    return socket.recv(b, off, len);
  }

  @Override
  public void close() {
    socket.close();
  }
}
