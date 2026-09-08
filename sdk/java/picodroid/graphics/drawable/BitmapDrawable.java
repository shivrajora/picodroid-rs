// SPDX-License-Identifier: GPL-3.0-only
package picodroid.graphics.drawable;

import picodroid.view.View;
import picodroid.widget.ImageView;

/**
 * An image from a package's bundled assets, as {@code PackageManager.getApplicationIcon} returns
 * it. Mirrors {@code android.graphics.drawable.BitmapDrawable}: show it with {@link
 * ImageView#setImageDrawable}, or use it as a background with {@link View#setBackground}.
 *
 * <p>Apps do not build these themselves (there is no {@code Bitmap} class); for the app's own
 * assets use {@link ImageView#setImageSource}.
 */
public class BitmapDrawable extends Drawable {
  /** Framework image handle: an entry in the runtime's icon registry, valid while this app runs. */
  private final int imageHandle;

  /** Framework use: {@code PackageManager} wraps a registered icon. */
  public BitmapDrawable(int imageHandle) {
    this.imageHandle = imageHandle;
  }

  @Override
  public void applyTo(View v) {
    if (v instanceof ImageView) {
      nativeSetImageSrc(v, imageHandle);
    } else {
      nativeSetBackground(v, imageHandle);
    }
  }

  private static native void nativeSetImageSrc(View target, int imageHandle);

  private static native void nativeSetBackground(View target, int imageHandle);
}
