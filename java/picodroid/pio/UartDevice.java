package picodroid.pio;

public class UartDevice {
    public static final int PARITY_NONE = 0;
    public static final int PARITY_EVEN = 1;
    public static final int PARITY_ODD  = 2;

    public static final int HW_FLOW_CONTROL_NONE        = 0;
    public static final int HW_FLOW_CONTROL_AUTO_RTSCTS = 1;

    private int uartId;

    // Package-private — created via PeripheralManager.openUartDevice()
    UartDevice(int uartId) {
        this.uartId = uartId;
    }

    public native void setBaudrate(int rate);
    public native void setDataSize(int size);
    public native void setParity(int mode);
    public native void setStopBits(int bits);
    public native void setHardwareFlowControl(int mode);
    /** Write a single byte. Returns 1 on success. */
    public native int writeByte(int b);
    /** Read a single byte. Returns -1 if the RX FIFO is empty. */
    public native int readByte();
    public native void close();
}
