// SPDX-License-Identifier: GPL-3.0-only
package picodroid.view.animation;

/**
 * Mirrors {@code android.view.animation.AccelerateDecelerateInterpolator}: ease-in-out (3t²−2t³).
 */
public class AccelerateDecelerateInterpolator implements Interpolator {
  @Override
  public float getInterpolation(float input) {
    return input * input * (3.0f - 2.0f * input);
  }
}
