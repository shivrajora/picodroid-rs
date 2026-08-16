// SPDX-License-Identifier: GPL-3.0-only
package picodroid.net;

import java.io.IOException;
import java.net.SocketException;

/** UDP socket — send and receive datagrams. */
public class DatagramSocket implements AutoCloseable {
  private int handle;

  /**
   * Create a UDP socket bound to a local port.
   *
   * @param localPort local port to bind (0 for any available port)
   * @throws java.net.BindException if the port is already in use
   * @throws SocketException for any other bind failure
   */
  public DatagramSocket(int localPort) throws SocketException {
    this.handle = nativeCreate(localPort);
  }

  /**
   * Send a datagram packet to the address/port specified in the packet.
   *
   * @throws IOException if the send fails
   */
  public native void send(DatagramPacket packet) throws IOException;

  /**
   * Receive a datagram packet (blocking). Fills packet's data, length, address, and port.
   *
   * @throws java.net.SocketTimeoutException if a timeout set via {@link #setTimeout} expired
   * @throws IOException for any other receive failure
   */
  public native void receive(DatagramPacket packet) throws IOException;

  /** Set receive timeout in milliseconds (0 = infinite). */
  public native void setTimeout(int millis);

  @Override
  public native void close();

  private static native int nativeCreate(int localPort) throws SocketException;
}
