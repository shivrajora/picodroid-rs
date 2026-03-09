package picodroid.pio;

public class Gpio {
    public static final int DIRECTION_OUT_INITIALLY_HIGH = 1;
    public static final int DIRECTION_OUT_INITIALLY_LOW  = 2;

    private int pin;

    // Package-private — created via PeripheralManager.openGpio()
    Gpio(int pin) {
        this.pin = pin;
    }

    public native void setDirection(int direction);
    public native void setValue(boolean value);
    public native void close();
}
