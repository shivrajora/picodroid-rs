// SPDX-License-Identifier: GPL-3.0-only
package picodroid.net;

/** TCP server socket — binds to a port and accepts incoming connections. */
public class ServerSocket implements AutoCloseable {
  private int handle;

  /**
   * Create a server socket listening on the given port.
   *
   * @param port local port to bind and listen on
   */
  public ServerSocket(int port) {
    this.handle = nativeListen(port);
  }

  /**
   * Accept an incoming connection (blocking).
   *
   * @return a new Socket for the accepted client
   */
  public native Socket accept();

  @Override
  public native void close();

  private static native int nativeListen(int port);
}
