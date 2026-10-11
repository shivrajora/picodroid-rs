// SPDX-License-Identifier: GPL-3.0-only
package picodroid.net;

/**
 * A UDP datagram: a byte buffer with the window ({@code offset}, {@code length}) the datagram
 * occupies in it, and the remote address and port. Mirrors {@code java.net.DatagramPacket}: {@link
 * DatagramSocket#send} sends {@code length} bytes from {@code offset}, {@link
 * DatagramSocket#receive} fills the window and sets {@code length} to what arrived (a longer
 * datagram is cut to the window).
 */
public class DatagramPacket {
  // Read by the runtime by slot, in this order: data, length, address, port, offset, bufLength.
  private byte[] data;
  private int length;
  private int address;
  private int port;
  private int offset;
  // The window's full size, so a packet reused across receives keeps it after a short datagram
  // set `length` to the count received (java.net.DatagramPacket's bufLength).
  private int bufLength;

  /** A packet for receiving into {@code buf[0..length)} (address and port set by receive). */
  public DatagramPacket(byte[] buf, int length) {
    this(buf, 0, length);
  }

  /** A packet for receiving into {@code buf[offset..offset+length)}. */
  public DatagramPacket(byte[] buf, int offset, int length) {
    setData(buf, offset, length);
  }

  /** A packet carrying {@code buf[offset..offset+length)} to {@code address:port}. */
  public DatagramPacket(byte[] buf, int offset, int length, InetAddress address, int port) {
    this(buf, offset, length);
    setAddress(address);
    setPort(port);
  }

  /** A packet carrying {@code buf[0..length)} to {@code address:port}. */
  public DatagramPacket(byte[] buf, int length, InetAddress address, int port) {
    this(buf, 0, length, address, port);
  }

  /** A packet carrying {@code buf[0..length)} to the raw IPv4 {@code address} and {@code port}. */
  public DatagramPacket(byte[] buf, int length, int address, int port) {
    this(buf, 0, length);
    this.address = address;
    setPort(port);
  }

  public byte[] getData() {
    return data;
  }

  /** Where the datagram starts in {@link #getData()}. */
  public int getOffset() {
    return offset;
  }

  /** Bytes to send, or bytes received. */
  public int getLength() {
    return length;
  }

  /** The window's full size: what the next {@code receive} may fill. */
  int bufferLength() {
    return bufLength;
  }

  /** Use {@code buf[offset..offset+length)} as the datagram's window. */
  public void setData(byte[] buf, int offset, int length) {
    if (buf == null) {
      throw new NullPointerException("null packet buffer");
    }
    if (offset < 0 || length < 0 || offset > buf.length - length) {
      throw new IllegalArgumentException("illegal offset or length");
    }
    this.data = buf;
    this.offset = offset;
    this.length = length;
    this.bufLength = length;
  }

  /** Use the whole of {@code buf} as the datagram's window. */
  public void setData(byte[] buf) {
    if (buf == null) {
      throw new NullPointerException("null packet buffer");
    }
    setData(buf, 0, buf.length);
  }

  /** Set the window's length; it must still fit the buffer from {@link #getOffset()}. */
  public void setLength(int length) {
    if (length < 0 || length > data.length - offset) {
      throw new IllegalArgumentException("illegal length");
    }
    this.length = length;
    this.bufLength = length;
  }

  public InetAddress getAddress() {
    return new InetAddress(address);
  }

  public void setAddress(InetAddress address) {
    this.address = address == null ? 0 : address.getRawAddress();
  }

  public int getPort() {
    return port;
  }

  public void setPort(int port) {
    if (port < 0 || port > 0xFFFF) {
      throw new IllegalArgumentException("port out of range: " + port);
    }
    this.port = port;
  }
}
