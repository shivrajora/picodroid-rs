// SPDX-License-Identifier: GPL-3.0-only
package fragmentdemo;

import picodroid.app.Fragment;
import picodroid.os.Bundle;
import picodroid.view.LayoutInflater;
import picodroid.view.View;
import picodroid.view.ViewGroup;

/** A fragment without a UI: {@code onCreateView} returns null, as Android allows. */
public class HeadlessFragment extends Fragment {
  static int instances;
  final int n;

  public HeadlessFragment() {
    n = ++instances;
  }

  private void mark(String event) {
    T.log("Headless" + n + "." + event);
  }

  @Override
  public View onCreateView(
      LayoutInflater inflater, ViewGroup container, Bundle savedInstanceState) {
    mark("onCreateView");
    return null;
  }

  @Override
  public void onResume() {
    super.onResume();
    mark("onResume");
  }

  @Override
  public void onHiddenChanged(boolean hidden) {
    super.onHiddenChanged(hidden);
    mark(hidden ? "onHiddenChanged(true)" : "onHiddenChanged(false)");
  }

  @Override
  public void onDestroyView() {
    mark("onDestroyView");
    super.onDestroyView();
  }

  @Override
  public void onDestroy() {
    mark("onDestroy");
    super.onDestroy();
  }

  @Override
  public void onDetach() {
    mark("onDetach");
    super.onDetach();
  }
}
