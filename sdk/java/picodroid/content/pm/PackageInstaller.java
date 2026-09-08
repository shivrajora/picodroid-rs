// SPDX-License-Identifier: GPL-3.0-only
package picodroid.content.pm;

/**
 * Removes installed apps, the uninstall half of {@code android.content.pm.PackageInstaller}; obtain
 * it from {@link PackageManager#getPackageInstaller}. Android reports the outcome through an {@code
 * IntentSender}; Picodroid's call is synchronous and returns when the app is gone. Multi-app boards
 * only.
 */
public class PackageInstaller {
  private static final PackageInstaller INSTANCE = new PackageInstaller();

  private PackageInstaller() {}

  static PackageInstaller getInstance() {
    return INSTANCE;
  }

  /**
   * Erase the named app and its data. The app that calls this keeps running. Throws {@link
   * IllegalArgumentException} when the package is not installed, is a system app, or is the app
   * making the call; {@link IllegalStateException} when the device could not erase it.
   */
  public void uninstall(String packageName) {
    int code = nativeUninstall(packageName);
    if (code == 0) {
      return;
    }
    if (code == 1) {
      throw new IllegalArgumentException("not installed: " + packageName);
    }
    if (code == 2) {
      throw new IllegalArgumentException("system package: " + packageName);
    }
    if (code == 3) {
      throw new IllegalArgumentException("running: " + packageName);
    }
    throw new IllegalStateException("uninstall failed: " + packageName);
  }

  /** 0 done, 1 not installed, 2 a system app, 3 the running app, 4 the platform could not. */
  private static native int nativeUninstall(String packageName);
}
