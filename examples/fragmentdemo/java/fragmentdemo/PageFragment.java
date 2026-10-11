// SPDX-License-Identifier: GPL-3.0-only
package fragmentdemo;

import picodroid.app.Fragment;
import picodroid.os.Bundle;
import picodroid.view.LayoutInflater;
import picodroid.view.View;
import picodroid.view.ViewGroup;
import picodroid.widget.TextView;

/** A ViewPager2 page: one label naming the item it shows. */
public class PageFragment extends Fragment {
  final long itemId;

  public PageFragment() {
    this(-1);
  }

  PageFragment(long itemId) {
    this.itemId = itemId;
  }

  @Override
  public View onCreateView(
      LayoutInflater inflater, ViewGroup container, Bundle savedInstanceState) {
    TextView label = new TextView(requireContext());
    label.setText("item " + itemId);
    return label;
  }
}
