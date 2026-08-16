// SPDX-License-Identifier: GPL-3.0-only
package picodroid.net;

import java.io.IOException;

/** TCP server socket — binds to a port and accepts incoming connections. */
public class ServerSocket implements AutoCloseable {
  private int handle;

  /**
   * Create a server socket listening on the given port.
   *
   * @param port local port to bind and listen on
   * @throws java.net.BindException if the port is already in use
   * @throws IOException for any other bind/listen failure
   */
  public ServerSocket(int port) throws IOException {
    this.handle = nativeListen(port);
  }

  /**
   * Accept an incoming connection (blocking).
   *
   * @return a new Socket for the accepted client
   * @throws java.net.SocketTimeoutException if a timeout set via {@link #setSoTimeout} expired
   * @throws IOException for any other accept failure
   */
  public native Socket accept() throws IOException;

  /**
   * Set the accept timeout in milliseconds (0 = infinite), as {@code
   * java.net.ServerSocket.setSoTimeout}.
   *
   * @throws java.net.SocketException if the socket is closed
   */
  public native void setSoTimeout(int millis) throws java.net.SocketException;

  @Override
  public native void close();

  private static native int nativeListen(int port) throws IOException;
}
