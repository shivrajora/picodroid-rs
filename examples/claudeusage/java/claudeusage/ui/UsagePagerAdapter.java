// SPDX-License-Identifier: GPL-3.0-only
package claudeusage.ui;

import claudeusage.R;
import picodroid.app.Activity;
import picodroid.app.Fragment;
import picodroid.widget.FragmentStateAdapter;

/** The four data screens, in page order: Limits, Models, Burn rate, History. */
final class UsagePagerAdapter extends FragmentStateAdapter {
  static final int COUNT = 4;

  UsagePagerAdapter(Activity activity) {
    super(activity);
  }

  @Override
  public int getItemCount() {
    return COUNT;
  }

  @Override
  public Fragment createFragment(int position) {
    switch (position) {
      case 1:
        return new ModelsPage();
      case 2:
        return new BurnPage();
      case 3:
        return new HistoryPage();
      default:
        return new LimitsPage();
    }
  }

  /** The header title of page {@code position}. */
  static int titleRes(int position) {
    switch (position) {
      case 1:
        return R.string.page_models;
      case 2:
        return R.string.page_burn;
      case 3:
        return R.string.page_history;
      default:
        return R.string.page_limits;
    }
  }
}
