package arraydemo;

import picodroid.pio.PeripheralManager;
import picodroid.pio.UartDevice;
import picodroid.util.Log;

public class ArrayDemo {
  public static void main(String[] args) {
    // Allocate a byte array and fill it with 'A'..'P'
    byte[] buf = new byte[16];
    for (int i = 0; i < buf.length; i++) {
      buf[i] = (byte) (0x41 + i);
    }

    // Open UART and send the array contents
    PeripheralManager mgr = PeripheralManager.getInstance();
    UartDevice uart = mgr.openUartDevice("UART0");
    uart.setBaudrate(115200);
    uart.setDataSize(8);
    uart.setParity(UartDevice.PARITY_NONE);
    uart.setStopBits(1);
    uart.setHardwareFlowControl(UartDevice.HW_FLOW_CONTROL_NONE);

    Log.i("ARRAY", "Sending " + buf.length + " bytes");
    for (int i = 0; i < buf.length; i++) {
      uart.writeByte(buf[i] & 0xFF);
    }
  }
}
