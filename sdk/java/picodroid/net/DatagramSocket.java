// SPDX-License-Identifier: GPL-3.0-only
package picodroid.net;

import java.io.IOException;
import java.net.SocketException;

/** UDP socket — send and receive datagrams. */
public class DatagramSocket implements AutoCloseable {
  // The natives read this at field slot 0 (net/fields.rs): declare nothing above it.
  private int handle;

  /** Java's default: a fresh socket may send to a broadcast address. */
  private boolean broadcast = true;

  /**
   * Create a UDP socket bound to any available local port.
   *
   * @throws SocketException if the socket cannot be opened
   */
  public DatagramSocket() throws SocketException {
    this(0);
  }

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
   * @throws java.net.SocketTimeoutException if a timeout set via {@link #setSoTimeout} expired
   * @throws IOException for any other receive failure
   */
  public native void receive(DatagramPacket packet) throws IOException;

  /**
   * Set the receive timeout in milliseconds (0 = infinite), as {@code java.net.DatagramSocket}
   * spells it: a {@link #receive} still waiting when it expires throws {@code
   * java.net.SocketTimeoutException}.
   */
  public void setSoTimeout(int millis) {
    setTimeout(millis);
  }

  /** The older spelling of {@link #setSoTimeout}. */
  public native void setTimeout(int millis);

  /**
   * Allow or forbid sending to a broadcast address (255.255.255.255 or the subnet's), as {@code
   * java.net.DatagramSocket.setBroadcast}. On by default, as in Java. Picodroid's network stacks
   * permit broadcast unconditionally, so the flag is recorded for API parity rather than enforced.
   */
  public void setBroadcast(boolean on) {
    broadcast = on;
  }

  /** Whether {@link #setBroadcast} is on. */
  public boolean getBroadcast() {
    return broadcast;
  }

  @Override
  public native void close();

  private static native int nativeCreate(int localPort) throws SocketException;
}
