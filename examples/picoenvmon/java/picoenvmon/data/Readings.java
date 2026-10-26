// SPDX-License-Identifier: GPL-3.0-only
package picoenvmon.data;

/**
 * One second's smoothed values, as {@link LatestReadings#readings()} publishes them: immutable, so
 * an observer may keep it. Indexed by {@link LatestReadings#IDX_TEMPERATURE} and its siblings.
 */
public final class Readings {
  private final float[] values;
  private final int validMask;

  Readings(float[] values, int validMask) {
    this.values = values;
    this.validMask = validMask;
  }

  /** Whether {@code idx} has received at least one sample. */
  public boolean isValid(int idx) {
    return (validMask & (1 << idx)) != 0;
  }

  public float get(int idx) {
    return values[idx];
  }
}
