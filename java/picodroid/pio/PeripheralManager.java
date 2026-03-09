package picodroid.pio;

public class PeripheralManager {
    private PeripheralManager() {}

    public static native PeripheralManager getInstance();

    public native Gpio openGpio(String name);
}
