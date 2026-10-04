// SPDX-License-Identifier: GPL-3.0-only
package picodroid.net;

/** A UDP datagram packet — holds data, length, address, and port. */
public class DatagramPacket {
  private byte[] data;
  private int length;
  private int address;
  private int port;

  /** Create a packet for receiving (address/port filled by receive()). */
  public DatagramPacket(byte[] data, int length) {
    this.data = data;
    this.length = length;
  }

  /** Create a packet for sending to a specific destination. */
  public DatagramPacket(byte[] data, int length, int address, int port) {
    this.data = data;
    this.length = length;
    this.address = address;
    this.port = port;
  }

  /** Create a packet for sending to {@code address:port}, as {@code java.net.DatagramPacket}. */
  public DatagramPacket(byte[] data, int length, InetAddress address, int port) {
    this(data, length, address.getRawAddress(), port);
  }

  public byte[] getData() {
    return data;
  }

  public int getLength() {
    return length;
  }

  public void setLength(int length) {
    this.length = length;
  }

  /**
   * Mirrors {@code java.net.DatagramPacket#getAddress()}: where this packet is going, or after a
   * {@code receive} where it came from.
   */
  public InetAddress getAddress() {
    return new InetAddress(address);
  }

  public int getPort() {
    return port;
  }
}
