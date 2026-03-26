package picodroid.pio;

public class I2cDevice implements AutoCloseable {
  public static final int SPEED_STANDARD = 100000;
  public static final int SPEED_FAST = 400000;

  private int i2cId;

  // Package-private — created via PeripheralManager.openI2cDevice()
  I2cDevice(int i2cId) {
    this.i2cId = i2cId;
  }

  /** Set bus speed in Hz (e.g. SPEED_STANDARD or SPEED_FAST). */
  public native void setSpeed(int hz);

  /**
   * Blocking write of data[0..len-1] to the device at the given 7-bit address. A STOP condition is
   * issued after the last byte. Returns the number of bytes written, or -1 on NACK/abort.
   */
  public native int write(int address, byte[] data, int len);

  /**
   * Blocking read of len bytes from the device at the given 7-bit address into buf[0..len-1].
   * Returns the number of bytes read, or -1 on NACK/abort.
   */
  public native int read(int address, byte[] buf, int len);

  public native void close();
}
