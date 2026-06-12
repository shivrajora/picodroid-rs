// SPDX-License-Identifier: GPL-3.0-only
package java.lang;

public final class System {
  private System() {}

  public static native long currentTimeMillis();

  /**
   * Copies {@code length} elements from {@code src[srcPos]} to {@code dest[destPos]}. Mirrors
   * {@code java.lang.System#arraycopy}: element types must match (ArrayStoreException), ranges must
   * be in bounds (IndexOutOfBoundsException), and overlapping self-copies are safe.
   */
  public static native void arraycopy(Object src, int srcPos, Object dest, int destPos, int length);
}
