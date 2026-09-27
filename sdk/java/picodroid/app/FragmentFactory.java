// SPDX-License-Identifier: GPL-3.0-only
package picodroid.app;

/**
 * Creates fragment instances by class name when a {@link FragmentManager} restores the fragments a
 * destroyed Activity had. Mirrors {@code androidx.fragment.app.FragmentFactory}, minus the {@code
 * ClassLoader} parameter: there is no reflection on this runtime, so the default cannot construct
 * anything and an app that wants its fragments back after a re-creation installs its own before
 * {@code super.onCreate}:
 *
 * <pre>{@code
 * getSupportFragmentManager().setFragmentFactory(new FragmentFactory() {
 *   @Override public Fragment instantiate(String className) {
 *     if (className.equals(HomeFragment.class.getName())) return new HomeFragment();
 *     if (className.equals(DetailFragment.class.getName())) return new DetailFragment();
 *     return super.instantiate(className);
 *   }
 * });
 * super.onCreate(savedInstanceState);
 * }</pre>
 *
 * <p>Compare with {@code X.class.getName()}, never a string literal: a shrunk build renames app
 * classes, and {@code getName()} follows the rename while a literal does not.
 */
public class FragmentFactory {
  public FragmentFactory() {}

  /**
   * Return a new instance of the fragment class named {@code className} (as {@code
   * getClass().getName()} spelled it when the state was saved). The default throws.
   */
  public Fragment instantiate(String className) {
    throw new IllegalStateException(
        "No FragmentFactory for "
            + className
            + ": override FragmentFactory.instantiate and setFragmentFactory before"
            + " super.onCreate");
  }
}
