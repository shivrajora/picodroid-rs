package picodroid.graphics.drawable;

import picodroid.view.View;

/**
 * Base for objects that paint a View's background. Subclasses describe a bundle of visual
 * properties (fill color, corner radius, stroke, gradient) and are applied to a View via {@link
 * View#setBackground}.
 *
 * <p>Unlike Android's {@code Drawable}, picodroid's drawable doesn't own its own draw loop — it
 * just applies a set of LVGL style properties to the target widget. This keeps the v1 abstraction
 * thin; richer drawables (layer-list, animated, bitmap-tiled) are planned follow-ups.
 */
public abstract class Drawable {
  /**
   * Apply this drawable's visual properties to {@code v}. Called by {@link View#setBackground}.
   * Subclasses must override.
   */
  public abstract void applyTo(View v);
}
