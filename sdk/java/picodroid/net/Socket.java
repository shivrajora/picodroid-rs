// SPDX-License-Identifier: GPL-3.0-only
package picodroid.net;

import java.io.IOException;

/** TCP client socket. */
public class Socket implements AutoCloseable {
  private int handle;

  /**
   * Create an unconnected TCP socket. Call connect() to establish a connection.
   *
   * <p>Like {@code java.net.Socket()}, this constructor declares no checked exceptions; under
   * resource exhaustion it can still fail with a runtime-delivered IOException.
   */
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
   * @throws java.net.ConnectException if the peer actively refused the connection
   * @throws java.net.SocketTimeoutException if the connect attempt timed out (unreachable hosts
   *     surface here too — the SYN retries exhaust without an answer)
   * @throws IOException for any other failure; the message carries the stack error code
   */
  public native void connect(int addr, int port) throws IOException;

  /**
   * Send data.
   *
   * @return number of bytes sent
   * @throws java.net.SocketException if the connection was reset or the socket is closed
   * @throws IOException for any other send failure
   */
  public native int send(byte[] data, int offset, int len) throws IOException;

  /**
   * Receive data (blocking).
   *
   * @return number of bytes received, or -1 at orderly end of stream — never -1 for errors
   * @throws java.net.SocketTimeoutException if a timeout set via {@link #setTimeout} expired
   * @throws java.net.SocketException if the socket is closed
   * @throws IOException for any other receive failure
   */
  public native int recv(byte[] buf, int offset, int len) throws IOException;

  /** Set receive timeout in milliseconds (0 = infinite). */
  public native void setTimeout(int millis);

  @Override
  public native void close();

  private static native int nativeCreate();
}
