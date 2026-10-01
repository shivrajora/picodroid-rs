// SPDX-License-Identifier: GPL-3.0-only
package picodroid.lifecycle;

/**
 * A {@link ViewModelStoreOwner} that names the factory a {@link ViewModelProvider} built without
 * one uses; mirrors {@code androidx.lifecycle.HasDefaultViewModelProviderFactory}. An {@link
 * picodroid.app.Activity} is one: override {@link #getDefaultViewModelProviderFactory} there and
 * its fragments write {@code new ViewModelProvider(requireActivity()).get(X.class)}, as on Android.
 */
public interface HasDefaultViewModelProviderFactory {
  /**
   * The factory, or {@code null} for none. Android's default one reflects on the class; there is no
   * reflection here, so the default is none.
   */
  ViewModelProvider.Factory getDefaultViewModelProviderFactory();
}
