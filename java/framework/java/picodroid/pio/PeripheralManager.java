package picodroid.pio;

public class PeripheralManager {
  private PeripheralManager() {}

  public static native PeripheralManager getInstance();

  public native Gpio openGpio(String name);

  public native UartDevice openUartDevice(String name);

  public native I2cDevice openI2cDevice(String name);

  public native SpiDevice openSpiDevice(String name);
}
