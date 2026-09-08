// SPDX-License-Identifier: GPL-3.0-only
package picodroid.content.pm;

import java.util.ArrayList;
import java.util.List;
import picodroid.content.Intent;
import picodroid.graphics.drawable.BitmapDrawable;
import picodroid.graphics.drawable.Drawable;

/**
 * What this device has and what it can run. Mirrors {@code android.content.pm.PackageManager}:
 * {@link #hasSystemFeature} for the hardware, and on a multi-app board the query methods that a
 * launcher needs — every installed package, its label and icon, and an Intent that starts it.
 *
 * <p>On a single-app board only {@link #hasSystemFeature} is served; the other methods are not part
 * of that board's framework.
 */
public class PackageManager {
  /** Feature name for {@link #hasSystemFeature}: the board has a WiFi link. */
  public static final String FEATURE_WIFI = "picodroid.hardware.wifi";

  /** Feature name for {@link #hasSystemFeature}: the board has an Ethernet link. */
  public static final String FEATURE_ETHERNET = "picodroid.hardware.ethernet";

  /** Thrown when a package name is not installed on this device. */
  public static class NameNotFoundException extends Exception {
    public NameNotFoundException() {}

    public NameNotFoundException(String name) {
      super(name);
    }
  }

  private static final PackageManager INSTANCE = new PackageManager();

  private PackageManager() {}

  public static PackageManager getInstance() {
    return INSTANCE;
  }

  /** The installer, which removes apps (multi-app boards only). */
  public PackageInstaller getPackageInstaller() {
    return PackageInstaller.getInstance();
  }

  /**
   * Whether the device has the named feature ({@link #FEATURE_WIFI}, {@link #FEATURE_ETHERNET}).
   */
  public native boolean hasSystemFeature(String name);

  /**
   * Every installed package, system apps included. {@code flags} is accepted for Android source
   * compatibility and ignored.
   */
  public List<PackageInfo> getInstalledPackages(int flags) {
    int count = nativeCount();
    List<PackageInfo> list = new ArrayList<PackageInfo>();
    for (int i = 0; i < count; i++) {
      list.add(infoAt(i));
    }
    return list;
  }

  /** The named package; {@code flags} is ignored. */
  public PackageInfo getPackageInfo(String packageName, int flags) throws NameNotFoundException {
    int i = nativeIndexOf(packageName);
    if (i < 0) {
      throw new NameNotFoundException(packageName);
    }
    return infoAt(i);
  }

  /**
   * An Intent that starts the named package, for {@code startActivity}; {@code null} when it is not
   * installed. The current app is torn down when the Intent is started: one app runs at a time.
   */
  public Intent getLaunchIntentForPackage(String packageName) {
    if (nativeIndexOf(packageName) < 0) {
      return null;
    }
    return new Intent().setPackage(packageName);
  }

  /** The app's display name (its manifest {@code label}), or its package name when it has none. */
  public CharSequence getApplicationLabel(ApplicationInfo info) {
    int i = nativeIndexOf(info.packageName);
    return i < 0 ? info.packageName : nativeLabel(i);
  }

  /**
   * The app's icon (its manifest {@code icon}, an image in its bundled assets), or {@code null}
   * when it has none. Show it with {@code ImageView.setImageDrawable}.
   */
  public Drawable getApplicationIcon(String packageName) throws NameNotFoundException {
    int i = nativeIndexOf(packageName);
    if (i < 0) {
      throw new NameNotFoundException(packageName);
    }
    return iconAt(i);
  }

  /** The app's icon, or {@code null} when it has none or the package is gone. */
  public Drawable getApplicationIcon(ApplicationInfo info) {
    int i = nativeIndexOf(info.packageName);
    return i < 0 ? null : iconAt(i);
  }

  private static PackageInfo infoAt(int i) {
    PackageInfo p = new PackageInfo();
    p.packageName = nativePackageName(i);
    p.versionName = nativeVersionName(i);
    p.versionCode = nativeVersionCode(i);
    ApplicationInfo a = new ApplicationInfo();
    a.packageName = p.packageName;
    a.flags = nativeIsSystem(i) ? ApplicationInfo.FLAG_SYSTEM : 0;
    p.applicationInfo = a;
    return p;
  }

  private static Drawable iconAt(int i) {
    int handle = nativeIconHandle(i);
    return handle < 0 ? null : new BitmapDrawable(handle);
  }

  // The package directory, one value per call (the runtime keeps no
  // strings): an index is the package's position in the directory, stable
  // while an app runs.
  private static native int nativeCount();

  private static native int nativeIndexOf(String packageName);

  private static native String nativePackageName(int index);

  private static native String nativeVersionName(int index);

  private static native int nativeVersionCode(int index);

  private static native String nativeLabel(int index);

  private static native boolean nativeIsSystem(int index);

  /** An image handle for {@link BitmapDrawable}, or -1 when the package has no icon. */
  private static native int nativeIconHandle(int index);
}
