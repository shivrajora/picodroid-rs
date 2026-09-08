// SPDX-License-Identifier: GPL-3.0-only
package picodroid.content.pm;

import picodroid.graphics.drawable.Drawable;

/**
 * The application half of a {@link PackageInfo}. Mirrors {@code
 * android.content.pm.ApplicationInfo}: the package name, the flags, and the label and icon loaders.
 */
public class ApplicationInfo {
  /** Set in {@link #flags} for a system app: one built into the firmware, such as the launcher. */
  public static final int FLAG_SYSTEM = 1;

  public String packageName;

  public int flags;

  public ApplicationInfo() {}

  public CharSequence loadLabel(PackageManager pm) {
    return pm.getApplicationLabel(this);
  }

  /** The app's icon, or {@code null} when it has none. */
  public Drawable loadIcon(PackageManager pm) {
    return pm.getApplicationIcon(this);
  }
}
