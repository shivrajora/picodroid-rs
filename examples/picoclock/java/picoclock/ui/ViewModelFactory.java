// SPDX-License-Identifier: GPL-3.0-only
package picoclock.ui;

import picoclock.AlarmStore;
import picodroid.lifecycle.ViewModel;
import picodroid.lifecycle.ViewModelProvider;

/**
 * Makes the screens' ViewModels that read or write the {@link AlarmStore}, the repository every
 * screen shares: Android's {@code ViewModelProvider.Factory} with the repository as its argument.
 */
final class ViewModelFactory implements ViewModelProvider.Factory {
  private final AlarmStore store;

  ViewModelFactory(AlarmStore store) {
    this.store = store;
  }

  @Override
  @SuppressWarnings("unchecked") // each branch builds exactly the class it compared against
  public <T extends ViewModel> T create(Class<T> modelClass) {
    if (modelClass == AlarmListViewModel.class) {
      return (T) new AlarmListViewModel(store);
    }
    if (modelClass == AlarmEditViewModel.class) {
      return (T) new AlarmEditViewModel(store);
    }
    if (modelClass == SetTimeViewModel.class) {
      return (T) new SetTimeViewModel(store);
    }
    if (modelClass == ClockViewModel.class) {
      return (T) new ClockViewModel();
    }
    throw new IllegalArgumentException("Unknown ViewModel class: " + modelClass.getName());
  }
}
