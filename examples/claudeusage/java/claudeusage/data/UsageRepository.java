// SPDX-License-Identifier: GPL-3.0-only
package claudeusage.data;

import picodroid.lifecycle.LiveData;
import picodroid.lifecycle.MutableLiveData;

/**
 * The app's one source of data: a {@link LiveData} of the current {@link UsageData}. {@link
 * UsageService} fills it from its poll thread's results; the screens' ViewModel observes it. It
 * holds no {@code Context}, so a ViewModel may keep it.
 *
 * <p>The value is set on the main thread only. {@link #refreshNow} and {@link #rediscover} may be
 * called from any thread.
 */
public final class UsageRepository {
  /** What fetches: the Service's poll thread, while the Service lives. */
  interface Poller {
    void refreshNow();

    void rediscover();
  }

  private static final UsageRepository INSTANCE = new UsageRepository();

  private final MutableLiveData<UsageData> data = new MutableLiveData<>(UsageData.NONE);

  private volatile Poller poller;

  private UsageRepository() {}

  public static UsageRepository getInstance() {
    return INSTANCE;
  }

  /** What is known now; never null. */
  public LiveData<UsageData> data() {
    return data;
  }

  /** Fetch now rather than at the next poll. */
  public void refreshNow() {
    Poller p = poller;
    if (p != null) {
      p.refreshNow();
    }
  }

  /**
   * Ask the LAN for the bridge again before fetching, whatever the last answer was: the PC may have
   * moved. Does nothing extra while an address is pinned.
   */
  public void rediscover() {
    Poller p = poller;
    if (p != null) {
      p.rediscover();
    }
  }

  void attach(Poller p) {
    poller = p;
  }

  void setData(UsageData value) {
    data.setValue(value);
  }
}
