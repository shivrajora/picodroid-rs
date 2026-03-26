package picodroid.pio;

public class SpiDevice implements AutoCloseable {
  public static final int MODE_0 = 0; // CPOL=0, CPHA=0
  public static final int MODE_1 = 1; // CPOL=0, CPHA=1
  public static final int MODE_2 = 2; // CPOL=1, CPHA=0
  public static final int MODE_3 = 3; // CPOL=1, CPHA=1

  private int spiId;

  // Package-private — created via PeripheralManager.openSpiDevice()
  SpiDevice(int spiId) {
    this.spiId = spiId;
  }

  /** Set SPI clock frequency in Hz (default: 1 000 000). */
  public native void setFrequency(int hz);

  /** Set SPI mode (0–3). Encodes CPOL (bit 1) and CPHA (bit 0). Default: MODE_0. */
  public native void setMode(int mode);

  /**
   * Full-duplex transfer: writes tx[0..len-1] and simultaneously stores received bytes in
   * rx[0..len-1]. Returns len on success.
   */
  public native int transfer(byte[] tx, byte[] rx, int len);

  /**
   * Write-only transfer: sends data[0..len-1] and discards received bytes. Returns len on success.
   */
  public native int write(byte[] data, int len);

  public native void close();
}
