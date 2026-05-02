// SPDX-License-Identifier: GPL-3.0-only
package spidemo;

import picodroid.app.Application;
import picodroid.pio.PeripheralManager;
import picodroid.pio.SpiDevice;
import picodroid.util.Log;

/**
 * SPI loopback demo.
 *
 * <p>Opens SPI0 (SCK=GP2, MOSI=GP3, MISO=GP0) at 1 MHz and sends bytes 0x00–0x0F in a full-duplex
 * transfer. If MISO is wired to MOSI (loopback), received bytes match sent bytes. Without loopback,
 * received bytes will be 0x00.
 */
public class SpiDemo extends Application {
  public void onCreate() {
    PeripheralManager pm = PeripheralManager.getInstance();
    SpiDevice spi = pm.openSpiDevice("SPI0");

    Log.i("SPI", "transfer start");
    byte[] tx = new byte[16];
    byte[] rx = new byte[16];
    for (int i = 0; i < 16; i++) {
      tx[i] = (byte) i;
    }
    int n = spi.transfer(tx, rx, 16);
    for (int i = 0; i < n; i++) {
      Log.i("SPI", "rx[" + i + "]=" + (rx[i] & 0xFF));
    }
    Log.i("SPI", "transfer done");
    spi.close();
  }
}
