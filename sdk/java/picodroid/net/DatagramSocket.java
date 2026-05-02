// SPDX-License-Identifier: GPL-3.0-only
package picodroid.net;

/** UDP socket — send and receive datagrams. */
public class DatagramSocket implements AutoCloseable {
  private int handle;

  /**
   * Create a UDP socket bound to a local port.
   *
   * @param localPort local port to bind (0 for any available port)
   */
  public DatagramSocket(int localPort) {
    this.handle = nativeCreate(localPort);
  }

  /** Send a datagram packet to the address/port specified in the packet. */
  public native void send(DatagramPacket packet);

  /** Receive a datagram packet (blocking). Fills packet's data, length, address, and port. */
  public native void receive(DatagramPacket packet);

  /** Set receive timeout in milliseconds (0 = infinite). */
  public native void setTimeout(int millis);

  public native void close();

  private static native int nativeCreate(int localPort);
}
