// SPDX-License-Identifier: GPL-3.0-only
package picodroid.net;

/** TCP client socket. */
public class Socket implements AutoCloseable {
  private int handle;

  /** Create an unconnected TCP socket. Call connect() to establish a connection. */
  public Socket() {
    this.handle = nativeCreate();
  }

  // Package-private: used by ServerSocket.accept() to wrap an already-connected handle.
  Socket(int handle) {
    this.handle = handle;
  }

  /**
   * Connect to a remote host.
   *
   * @param addr IPv4 address as a packed int (from InetAddress.getRawAddress())
   * @param port remote port number
   */
  public native void connect(int addr, int port);

  /**
   * Send data.
   *
   * @return number of bytes sent
   */
  public native int send(byte[] data, int offset, int len);

  /**
   * Receive data (blocking).
   *
   * @return number of bytes received, or -1 on error
   */
  public native int recv(byte[] buf, int offset, int len);

  /** Set receive timeout in milliseconds (0 = infinite). */
  public native void setTimeout(int millis);

  public native void close();

  private static native int nativeCreate();
}
