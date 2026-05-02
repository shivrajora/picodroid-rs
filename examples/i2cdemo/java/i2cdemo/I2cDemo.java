// SPDX-License-Identifier: GPL-3.0-only
package i2cdemo;

import picodroid.app.Application;
import picodroid.pio.I2cDevice;
import picodroid.pio.PeripheralManager;
import picodroid.util.Log;

/**
 * I2C bus scanner demo.
 *
 * <p>Opens I2C0 (SDA=GP4, SCL=GP5) and probes every 7-bit address in the standard range (0x08–0x77)
 * by attempting a zero-byte write. A device that acknowledges is logged; one that NACKs is skipped
 * silently.
 */
public class I2cDemo extends Application {
  public void onCreate() {
    PeripheralManager pm = PeripheralManager.getInstance();
    I2cDevice i2c = pm.openI2cDevice("I2C0");

    Log.i("I2C", "scan start");
    byte[] empty = new byte[0];
    for (int addr = 0x08; addr <= 0x77; addr++) {
      int result = i2c.write(addr, empty, 0);
      if (result >= 0) {
        Log.i("I2C", "found device at 0x" + addr);
      }
    }
    Log.i("I2C", "scan done");
    i2c.close();
  }
}
