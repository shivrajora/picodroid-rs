// SPDX-License-Identifier: GPL-3.0-only
package picodroid.content;

/**
 * Identifier for one application component: a package and a class within it. Mirrors {@code
 * android.content.ComponentName}; it is what a {@link ServiceConnection} is told the Service it is
 * connected to is called.
 */
public final class ComponentName {
  private final String mPackage;
  private final String mClass;

  /** The component {@code cls} (a Java class name, dot-separated) of package {@code pkg}. */
  public ComponentName(String pkg, String cls) {
    if (pkg == null) {
      throw new NullPointerException("package name is null");
    }
    if (cls == null) {
      throw new NullPointerException("class name is null");
    }
    mPackage = pkg;
    mClass = cls;
  }

  /** The component {@code cls} of the package {@code pkg} belongs to. */
  public ComponentName(Context pkg, Class<?> cls) {
    this(pkg.getPackageName(), cls.getName());
  }

  public String getPackageName() {
    return mPackage;
  }

  public String getClassName() {
    return mClass;
  }

  /** {@code package/class}, as Android spells a component in one string. */
  public String flattenToString() {
    return mPackage + "/" + mClass;
  }

  @Override
  public boolean equals(Object obj) {
    if (!(obj instanceof ComponentName)) {
      return false;
    }
    ComponentName other = (ComponentName) obj;
    return mPackage.equals(other.mPackage) && mClass.equals(other.mClass);
  }

  @Override
  public int hashCode() {
    return mPackage.hashCode() + mClass.hashCode();
  }

  @Override
  public String toString() {
    return "ComponentInfo{" + mPackage + "/" + mClass + "}";
  }
}
