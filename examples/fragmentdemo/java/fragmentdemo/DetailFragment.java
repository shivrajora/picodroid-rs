// SPDX-License-Identifier: GPL-3.0-only
package fragmentdemo;

import picodroid.app.Fragment;
import picodroid.content.Context;
import picodroid.os.Bundle;
import picodroid.view.LayoutInflater;
import picodroid.view.View;
import picodroid.view.ViewGroup;
import picodroid.widget.FrameLayout;
import picodroid.widget.TextView;

/**
 * A fragment that builds its view in code. Its first {@code onResume} also proves a transaction
 * cannot run from inside a lifecycle callback. Traces as {@code Detail<n>.<callback>}.
 */
public class DetailFragment extends Fragment {
  static int instances;
  final int n;
  private boolean resumedOnce;

  public DetailFragment() {
    n = ++instances;
  }

  private void mark(String event) {
    T.log("Detail" + n + "." + event);
  }

  @Override
  public void onAttach(Context context) {
    mark("onAttach");
  }

  @Override
  public void onCreate(Bundle savedInstanceState) {
    mark(savedInstanceState == null ? "onCreate(null)" : "onCreate(saved)");
  }

  @Override
  public View onCreateView(
      LayoutInflater inflater, ViewGroup container, Bundle savedInstanceState) {
    mark(savedInstanceState == null ? "onCreateView(null)" : "onCreateView(saved)");
    FrameLayout root = new FrameLayout(requireContext());
    TextView label = new TextView(requireContext());
    label.setText("Detail " + n + ", " + getString(R.string.detail_hint));
    root.addView(label);
    return root;
  }

  @Override
  public void onViewCreated(View view, Bundle savedInstanceState) {
    mark("onViewCreated");
  }

  @Override
  public void onStart() {
    mark("onStart");
  }

  @Override
  public void onResume() {
    mark("onResume");
    if (!resumedOnce) {
      resumedOnce = true;
      boolean refused = false;
      try {
        getParentFragmentManager().beginTransaction().commitNow();
      } catch (IllegalStateException e) {
        refused = true;
      }
      T.check("reentrant commitNow refused", refused);
    }
  }

  @Override
  public void onPause() {
    mark("onPause");
  }

  @Override
  public void onStop() {
    mark("onStop");
  }

  @Override
  public void onSaveInstanceState(Bundle outState) {
    mark("onSaveInstanceState");
  }

  @Override
  public void onHiddenChanged(boolean hidden) {
    mark(hidden ? "onHiddenChanged(true)" : "onHiddenChanged(false)");
  }

  @Override
  public void onDestroyView() {
    mark("onDestroyView");
  }

  @Override
  public void onDestroy() {
    mark("onDestroy");
  }

  @Override
  public void onDetach() {
    mark("onDetach");
  }
}
