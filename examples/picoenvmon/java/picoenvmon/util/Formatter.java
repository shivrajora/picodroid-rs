// SPDX-License-Identifier: GPL-3.0-only
package picoenvmon.util;

/** Sensor-value → display-string formatter. The °C↔°F flag is global to the app. */
public class Formatter {
  private boolean fahrenheit;

  public void toggleUnits() {
    fahrenheit = !fahrenheit;
  }

  public void setFahrenheit(boolean fahrenheit) {
    this.fahrenheit = fahrenheit;
  }

  public boolean isFahrenheit() {
    return fahrenheit;
  }

  public String formatTemp(float celsius) {
    if (fahrenheit) {
      int milliF = (int) (celsius * 1.8f * 1000) + 32_000;
      return centiToString(milliF / 10) + "F";
    }
    int centi = (int) (celsius * 100);
    return centiToString(centi) + "C";
  }

  public String formatHumidity(float pct) {
    return centiToString((int) (pct * 100)) + " %";
  }

  /** Pressure in hPa. */
  public String formatPressure(float hpa) {
    return centiToString((int) (hpa * 100)) + " hPa";
  }

  public String formatLux(float lux) {
    return Integer.toString((int) lux) + " lx";
  }

  /** Quick IAQ index from a gas-resistance reading. Higher gas resistance → cleaner air. */
  public String formatGasIaq(float gasOhm) {
    if (gasOhm <= 0f) {
      return "—";
    }
    int iaq = iaqFromGas(gasOhm);
    return Integer.toString(iaq) + " IAQ";
  }

  /**
   * 0..500 IAQ index, log-scaled around a 50 kΩ "average indoor" reference. Not a calibrated index;
   * useful as a comparative trend indicator only.
   */
  public static int iaqFromGas(float gasOhm) {
    if (gasOhm <= 1f) {
      return 500;
    }
    float ref = 50_000f;
    float ratio = gasOhm / ref;
    if (ratio <= 0.001f) {
      return 500;
    }
    if (ratio >= 4f) {
      return 0;
    }
    float log2 = log2(ratio);
    int iaq = 250 - (int) (log2 * 60f);
    if (iaq < 0) {
      iaq = 0;
    }
    if (iaq > 500) {
      iaq = 500;
    }
    return iaq;
  }

  private static float log2(float x) {
    int n = 0;
    float v = x;
    while (v > 1f) {
      v *= 0.5f;
      n++;
    }
    while (v < 0.5f) {
      v *= 2f;
      n--;
    }
    return (float) n + (v - 0.5f) * 2f - 0.25f;
  }

  /** "1234" → "12.34" — fixed two-decimal formatter without floats. */
  private static String centiToString(int centi) {
    boolean neg = centi < 0;
    int abs = neg ? -centi : centi;
    int whole = abs / 100;
    int frac = abs % 100;
    String fracStr = frac < 10 ? ("0" + frac) : Integer.toString(frac);
    String body = whole + "." + fracStr;
    return neg ? "-" + body : body;
  }
}
