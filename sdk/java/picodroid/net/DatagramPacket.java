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

  public byte[] getData() {
    return data;
  }

  public int getLength() {
    return length;
  }

  public void setLength(int length) {
    this.length = length;
  }

  public int getAddress() {
    return address;
  }

  public int getPort() {
    return port;
  }
}
