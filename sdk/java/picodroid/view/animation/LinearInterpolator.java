// SPDX-License-Identifier: GPL-3.0-only
package picodroid.view.animation;

/** Mirrors {@code android.view.animation.LinearInterpolator}. */
public class LinearInterpolator implements Interpolator {
  @Override
  public float getInterpolation(float input) {
    return input;
  }
}
