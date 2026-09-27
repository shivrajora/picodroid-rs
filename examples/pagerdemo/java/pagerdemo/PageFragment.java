// SPDX-License-Identifier: GPL-3.0-only
package pagerdemo;

import picodroid.app.Fragment;
import picodroid.os.Bundle;
import picodroid.util.Log;
import picodroid.view.LayoutInflater;
import picodroid.view.View;
import picodroid.view.ViewGroup;
import picodroid.widget.FrameLayout;
import picodroid.widget.TextView;

/**
 * One page: its position comes in as an argument (the Android idiom for adapter-made fragments),
 * and it counts its resumes in a saved int, so a page turned away from and back to shows how many
 * times it was visited across the pager's saved state and the host's re-creation.
 */
public class PageFragment extends Fragment {
  private static final int[] COLORS = {0xFF1F3A5F, 0xFF3A5F1F, 0xFF5F1F3A};

  private int position;
  private int visits;
  private TextView visitsLabel;

  static PageFragment newInstance(int position) {
    PageFragment f = new PageFragment();
    Bundle args = new Bundle();
    args.putInt("position", position);
    f.setArguments(args);
    return f;
  }

  @Override
  public void onCreate(Bundle savedInstanceState) {
    position = requireArguments().getInt("position");
    if (savedInstanceState != null) {
      visits = savedInstanceState.getInt("visits", 0);
    }
  }

  @Override
  public View onCreateView(
      LayoutInflater inflater, ViewGroup container, Bundle savedInstanceState) {
    FrameLayout root = new FrameLayout(requireContext());
    root.setLayoutParams(
        new ViewGroup.LayoutParams(
            ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.MATCH_PARENT));
    root.setBackgroundColor(COLORS[position]);
    TextView title = new TextView(requireContext());
    title.setText("Page " + position);
    root.addView(title);
    visitsLabel = new TextView(requireContext());
    visitsLabel.setPosition(0, 24);
    root.addView(visitsLabel);
    return root;
  }

  @Override
  public void onResume() {
    visits++;
    visitsLabel.setText("visits " + visits);
    PagerDemoActivity.lastVisits[position] = visits;
    Log.i(PagerDemoActivity.TAG, "page " + position + " resumed visits=" + visits);
  }

  @Override
  public void onSaveInstanceState(Bundle outState) {
    outState.putInt("visits", visits);
  }

  @Override
  public void onDestroyView() {
    visitsLabel = null;
  }

  @Override
  public void onDestroy() {
    PagerDemoActivity.destroyed[position]++;
    Log.i(PagerDemoActivity.TAG, "page " + position + " destroyed");
  }
}
