// SPDX-License-Identifier: GPL-3.0-only
package picodroid.net;

import java.io.IOException;
import java.io.OutputStream;

/**
 * {@link Socket#getOutputStream()}: an {@link OutputStream} over {@link Socket#send}. {@code send}
 * takes at most 256 bytes per call and reports how many it took, so a write loops until every byte
 * is out; a reset or closed connection surfaces as {@code SocketException}. Closing the stream
 * closes the socket, as on Android.
 */
final class SocketOutputStream extends OutputStream {
  private final Socket socket;

  SocketOutputStream(Socket socket) {
    this.socket = socket;
  }

  @Override
  public void write(int b) throws IOException {
    byte[] one = new byte[1];
    one[0] = (byte) b;
    write(one, 0, 1);
  }

  @Override
  public void write(byte[] b, int off, int len) throws IOException {
    if (b == null) {
      throw new NullPointerException();
    }
    if (off < 0 || len < 0 || len > b.length - off) {
      throw new IndexOutOfBoundsException();
    }
    while (len > 0) {
      int sent = socket.send(b, off, len);
      if (sent <= 0) {
        throw new java.net.SocketException("send returned " + sent);
      }
      off += sent;
      len -= sent;
    }
  }

  @Override
  public void close() {
    socket.close();
  }
}
