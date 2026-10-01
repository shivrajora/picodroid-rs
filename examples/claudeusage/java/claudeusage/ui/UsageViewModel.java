// SPDX-License-Identifier: GPL-3.0-only
package claudeusage.ui;

import claudeusage.data.UsageService;
import claudeusage.data.UsageSnapshot;
import picodroid.lifecycle.LiveData;
import picodroid.lifecycle.MutableLiveData;
import picodroid.lifecycle.ViewModel;

/**
 * What the screens show, shared by the Activity and its pages: the Activity fills it from the bound
 * {@link UsageService}, and each page observes {@link #usage} for itself.
 *
 * <p>The value is the Service, the app's repository, published again whenever it has something new
 * or a second passed, and {@code null} while it is not bound. An Android app would publish an
 * immutable state object instead; here that is an allocation a second for the life of the app, and
 * the Service's numbers are confined to the main thread, where every observer runs.
 */
final class UsageViewModel extends ViewModel {
  private final MutableLiveData<UsageService> usage = new MutableLiveData<>();

  LiveData<UsageService> usage() {
    return usage;
  }

  /** From the Activity: the Service has news, a second passed, or ({@code null}) it is unbound. */
  void publish(UsageService repo) {
    usage.setValue(repo);
  }

  /** Whether there are limits to show: until then the status screen covers the pages. */
  boolean hasData() {
    UsageService repo = usage.getValue();
    UsageSnapshot s = repo == null ? null : repo.snapshot();
    return s != null && s.hasLimits();
  }
}
