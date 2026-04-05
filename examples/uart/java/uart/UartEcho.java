package uart;

import picodroid.app.Application;
import picodroid.pio.PeripheralManager;
import picodroid.pio.UartDevice;
import picodroid.util.Log;

public class UartEcho extends Application {
  public void onCreate() {
    PeripheralManager mgr = PeripheralManager.getInstance();
    UartDevice uart = mgr.openUartDevice("UART0");

    // null-check before use — showcases ifnull (0xC6) opcode support
    if (uart == null) {
      Log.i("UART", "UART0 unavailable");
      return;
    }

    uart.setBaudrate(115200);
    uart.setDataSize(8);
    uart.setParity(UartDevice.PARITY_NONE);
    uart.setStopBits(1);
    uart.setHardwareFlowControl(UartDevice.HW_FLOW_CONTROL_NONE);

    // Transmit a greeting on startup
    String msg = "Hello UART!\r\n";
    for (int i = 0; i < msg.length(); i++) {
      uart.writeByte((int) msg.charAt(i));
    }

    // Echo everything received back to sender
    while (true) {
      int b = uart.readByte();
      if (b != -1) {
        // switch on received byte — showcases lookupswitch opcode (0xab) support
        switch (b) {
          case 'p':
            Log.i("UART", "PING");
            uart.writeByte('P');
            uart.writeByte('O');
            uart.writeByte('N');
            uart.writeByte('G');
            uart.writeByte('\r');
            uart.writeByte('\n');
            break;
          case '\r':
            uart.writeByte('\r');
            uart.writeByte('\n');
            break;
          default:
            Log.i("UART", "Echo: " + (char) b);
            uart.writeByte(b);
            break;
        }
      }
    }
  }
}
