// SPDX-License-Identifier: GPL-3.0-only
package picodroid.app;

/**
 * Makes fragments by class name when the framework re-creates saved ones, mirroring {@code
 * androidx.fragment.app.FragmentFactory}. The default constructs the class through its public
 * no-argument constructor ({@code Class.forName(className).newInstance()}), as Android's does, so
 * an app only installs a factory ({@link FragmentManager#setFragmentFactory}, before {@code
 * super.onCreate}) for fragments that take constructor arguments. {@link #instantiate} takes the
 * class name alone, with no {@code ClassLoader}.
 */
public class FragmentFactory {
  public FragmentFactory() {}

  /**
   * Return a new instance of the fragment class named {@code className} (as {@code
   * getClass().getName()} spelled it when the state was saved).
   *
   * @throws RuntimeException when the class is not packed with the app or has no public no-argument
   *     constructor (Android's {@code Fragment.InstantiationException})
   */
  public Fragment instantiate(String className) {
    try {
      return (Fragment) Class.forName(className).newInstance();
    } catch (ClassNotFoundException e) {
      throw new RuntimeException(
          "Unable to instantiate fragment "
              + className
              + ": make sure class name exists, is public, and has an empty constructor that is"
              + " public");
    } catch (InstantiationException | IllegalAccessException e) {
      throw new RuntimeException(
          "Unable to instantiate fragment "
              + className
              + ": could not find Fragment constructor (override FragmentFactory.instantiate for"
              + " one that takes arguments)");
    }
  }
}
