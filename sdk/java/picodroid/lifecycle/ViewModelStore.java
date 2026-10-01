// SPDX-License-Identifier: GPL-3.0-only
package picodroid.lifecycle;

import java.util.ArrayList;

/**
 * The {@link ViewModel}s of one {@link ViewModelStoreOwner}, by key; mirrors {@code
 * androidx.lifecycle.ViewModelStore}. Apps reach it through a {@link ViewModelProvider}.
 */
public class ViewModelStore {
  // Two short lists rather than a HashMap: an owner holds one or two ViewModels.
  private final ArrayList<String> mKeys = new ArrayList<>(2);
  private final ArrayList<ViewModel> mModels = new ArrayList<>(2);

  public ViewModelStore() {}

  final void put(String key, ViewModel viewModel) {
    int i = indexOf(key);
    if (i >= 0) {
      ViewModel old = mModels.set(i, viewModel);
      if (old != null) {
        old.clear();
      }
    } else {
      mKeys.add(key);
      mModels.add(viewModel);
    }
  }

  final ViewModel get(String key) {
    int i = indexOf(key);
    return i >= 0 ? mModels.get(i) : null;
  }

  private int indexOf(String key) {
    for (int i = 0; i < mKeys.size(); i++) {
      if (mKeys.get(i).equals(key)) {
        return i;
      }
    }
    return -1;
  }

  /** Clears every ViewModel ({@link ViewModel#onCleared}) and forgets them. */
  public final void clear() {
    for (int i = 0; i < mModels.size(); i++) {
      mModels.get(i).clear();
    }
    mKeys.clear();
    mModels.clear();
  }
}
