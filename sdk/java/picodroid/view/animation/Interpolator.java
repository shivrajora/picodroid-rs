// SPDX-License-Identifier: GPL-3.0-only
package picodroid.view.animation;

/**
 * Mirrors {@code android.view.animation.Interpolator}: maps a linear 0..1 animation fraction to an
 * eased one. Picodroid divergence: the native animation engine cannot upcall into a custom Java
 * Interpolator per frame, so only the built-in concrete classes ({@link LinearInterpolator}, {@link
 * AccelerateInterpolator}, {@link DecelerateInterpolator}, {@link
 * AccelerateDecelerateInterpolator}) are honored natively; a custom implementation falls back to
 * linear with a warning. {@link #getInterpolation} is still defined for source fidelity.
 */
public interface Interpolator {
  float getInterpolation(float input);
}
