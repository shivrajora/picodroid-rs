// SPDX-License-Identifier: GPL-3.0-only
package picodroid.net;

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

  /** Return the raw 32-bit address for use with Socket.connect(). */
  public int getRawAddress() {
    return address;
  }

  /** Return a dotted-decimal string ("a.b.c.d"). */
  public native String getHostAddress();
}
