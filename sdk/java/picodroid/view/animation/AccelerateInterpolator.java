// SPDX-License-Identifier: GPL-3.0-only
package picodroid.view.animation;

/** Mirrors {@code android.view.animation.AccelerateInterpolator}: starts slow, ends fast (t²). */
public class AccelerateInterpolator implements Interpolator {
  @Override
  public float getInterpolation(float input) {
    return input * input;
  }
}
