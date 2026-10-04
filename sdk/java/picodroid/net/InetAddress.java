// SPDX-License-Identifier: GPL-3.0-only
package picodroid.net;

import java.net.UnknownHostException;

/** IPv4 address representation. */
public class InetAddress {
  private int address; // packed IPv4 (host byte order: MSB = first octet)

  public InetAddress(int address) {
    this.address = address;
  }

  /** Create an address from four octets: getByAddress(192, 168, 1, 1). */
  public static InetAddress getByAddress(int a, int b, int c, int d) {
    int addr = ((a & 0xFF) << 24) | ((b & 0xFF) << 16) | ((c & 0xFF) << 8) | (d & 0xFF);
    return new InetAddress(addr);
  }

  /**
   * Mirrors {@code java.net.InetAddress#getByAddress(byte[])}: the address whose four octets are
   * {@code addr}, in network order.
   *
   * @throws UnknownHostException if {@code addr} is not four bytes long (IPv4 only)
   */
  public static InetAddress getByAddress(byte[] addr) throws UnknownHostException {
    if (addr == null || addr.length != 4) {
      throw new UnknownHostException("addr is of illegal length");
    }
    return getByAddress(addr[0], addr[1], addr[2], addr[3]);
  }

  /** Mirrors {@code java.net.InetAddress#getAddress()}: the four octets, in network order. */
  public byte[] getAddress() {
    return new byte[] {
      (byte) (address >>> 24), (byte) (address >>> 16), (byte) (address >>> 8), (byte) address
    };
  }

  /**
   * Resolve a hostname (or dotted-quad literal, which never hits the network) to an address.
   *
   * @throws UnknownHostException if the host cannot be resolved
   */
  public static InetAddress getByName(String host) throws UnknownHostException {
    return new InetAddress(nativeResolve(host));
  }

  private static native int nativeResolve(String host) throws UnknownHostException;

  /** Return the raw 32-bit address for use with Socket.connect(). */
  public int getRawAddress() {
    return address;
  }

  /** Return a dotted-decimal string ("a.b.c.d"). */
  public native String getHostAddress();
}
