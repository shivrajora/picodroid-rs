// SPDX-License-Identifier: GPL-3.0-only
package picodroid.lifecycle;

/**
 * Something with a {@link Lifecycle}, mirroring {@code androidx.lifecycle.LifecycleOwner}: an
 * {@link picodroid.app.Activity}, or the view of a {@link picodroid.app.Fragment} (see {@code
 * Fragment.getViewLifecycleOwner()}). {@link LiveData#observe} takes one, and stops delivering when
 * its lifecycle is no longer started.
 */
public interface LifecycleOwner {
  Lifecycle getLifecycle();
}
