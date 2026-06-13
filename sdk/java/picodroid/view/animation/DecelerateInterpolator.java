// SPDX-License-Identifier: GPL-3.0-only
package picodroid.view.animation;

/** Mirrors {@code android.view.animation.DecelerateInterpolator}: starts fast, ends slow. */
public class DecelerateInterpolator implements Interpolator {
  @Override
  public float getInterpolation(float input) {
    return 1.0f - (1.0f - input) * (1.0f - input);
  }
}
