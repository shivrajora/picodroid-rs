package uart;

import picodroid.pio.PeripheralManager;
import picodroid.pio.UartDevice;

public class UartEcho {
    public static void main(String[] args) {
        PeripheralManager mgr = PeripheralManager.getInstance();
        UartDevice uart = mgr.openUartDevice("UART0");
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
                uart.writeByte(b);
            }
        }
    }
}
