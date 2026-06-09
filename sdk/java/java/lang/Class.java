// SPDX-License-Identifier: GPL-3.0-only
package java.lang;

/**
 * Run-time class metadata. One {@code Class} object exists per loaded class; the {@code
 * MyType.class} literal evaluates to that singleton, so reference equality identifies the class:
 *
 * <pre>{@code
 * Class<MyService> a = MyService.class;
 * Class<MyService> b = MyService.class;
 * assert a == b;
 * }</pre>
 *
 * The minimal surface here exposes only {@link #getName}; full reflection ({@code forName}, {@code
 * newInstance}, member discovery) is intentionally out of scope.
 */
public final class Class<T> {
  private String name;

  private Class() {}

  public native String getName();

  @Override
  public String toString() {
    return "class " + getName();
  }
}
