// SPDX-License-Identifier: GPL-3.0-only
package picodroid.lifecycle;

/**
 * Something that keeps {@link ViewModel}s, mirroring {@code
 * androidx.lifecycle.ViewModelStoreOwner}: an {@link picodroid.app.Activity}. A {@link
 * ViewModelProvider} takes one.
 */
public interface ViewModelStoreOwner {
  ViewModelStore getViewModelStore();
}
