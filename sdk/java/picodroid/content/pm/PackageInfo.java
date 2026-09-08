// SPDX-License-Identifier: GPL-3.0-only
package picodroid.content.pm;

/**
 * What the package manager knows about one installed package. Mirrors the fields of {@code
 * android.content.pm.PackageInfo} that a Picodroid manifest carries.
 */
public class PackageInfo {
  /** The manifest's {@code package}. */
  public String packageName;

  /** The manifest's {@code version}. */
  public String versionName;

  /** The manifest's {@code version-code}; 1 when the manifest sets none. */
  public int versionCode;

  public ApplicationInfo applicationInfo;

  public PackageInfo() {}

  public long getLongVersionCode() {
    return versionCode;
  }
}
