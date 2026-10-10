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
 * <p>Reflection-lite (docs/designs/class-newinstance-2026-10.md): {@link #getName}, {@link
 * #forName} by that name, and {@link #newInstance} through the no-argument constructor. That is
 * what the framework needs to construct a ViewModel, a Fragment or a custom view a layout names, as
 * Android does, and what an app wrote for Android can lean on; there is no {@code Constructor},
 * {@code Method} or {@code Field}, no member discovery and no access check (a package-private
 * constructor is constructed, where Android would throw {@code IllegalAccessException}).
 */
public final class Class<T> {
  private String name;

  private Class() {}

  public native String getName();

  /**
   * The class named {@code className} in binary form ({@code "com.example.Gauge"}, as {@link
   * #getName} spells it, so the round trip holds when the app's classes are renamed by the
   * shrinker), if it is loaded: every class packed with the app, and the framework's. No class
   * loading happens, since every class is in flash already.
   *
   * @throws ClassNotFoundException when no such class is packed, with the name as its message
   */
  public static native Class<?> forName(String className) throws ClassNotFoundException;

  /**
   * A new instance through the no-argument constructor, the class initialised first if it never
   * was. The constructor runs on the calling thread, and anything it throws propagates as is.
   *
   * @throws InstantiationException for an interface, an abstract class, a class with no no-argument
   *     constructor, or a class with no class file ({@code String.class})
   * @throws IllegalAccessException never: there are no access checks on this runtime
   */
  public native T newInstance() throws InstantiationException, IllegalAccessException;

  @Override
  public String toString() {
    return "class " + getName();
  }
}
