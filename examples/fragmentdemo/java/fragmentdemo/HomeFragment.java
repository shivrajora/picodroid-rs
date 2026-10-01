// SPDX-License-Identifier: GPL-3.0-only
package fragmentdemo;

import picodroid.app.Fragment;
import picodroid.content.Context;
import picodroid.os.Bundle;
import picodroid.view.LayoutInflater;
import picodroid.view.View;
import picodroid.view.ViewGroup;

/**
 * The Android shape for a layout-backed fragment: {@code Fragment(int)} inflates {@code
 * res/layout/fragment_home.xml}, {@code onViewCreated} finds its children. Carries one saved int
 * across the host's re-creation and traces every callback as {@code Home<n>.<callback>}.
 */
public class HomeFragment extends Fragment {
  static int instances;
  final int n;
  int counter;

  public HomeFragment() {
    super(R.layout.fragment_home);
    n = ++instances;
  }

  private void mark(String event) {
    T.log("Home" + n + "." + event);
  }

  @Override
  public void onAttach(Context context) {
    super.onAttach(context);
    mark("onAttach");
    T.check("attach has activity", getActivity() != null && context == getActivity());
  }

  @Override
  public void onCreate(Bundle savedInstanceState) {
    super.onCreate(savedInstanceState);
    mark(savedInstanceState == null ? "onCreate(null)" : "onCreate(saved)");
    if (savedInstanceState != null) {
      counter = savedInstanceState.getInt("counter", -1);
    }
  }

  @Override
  public View onCreateView(
      LayoutInflater inflater, ViewGroup container, Bundle savedInstanceState) {
    mark(savedInstanceState == null ? "onCreateView(null)" : "onCreateView(saved)");
    T.check("home given its container", container != null);
    return super.onCreateView(inflater, container, savedInstanceState);
  }

  @Override
  public void onViewCreated(View view, Bundle savedInstanceState) {
    super.onViewCreated(view, savedInstanceState);
    mark("onViewCreated");
    T.check("home layout inflated", view.findViewById(R.id.home_line1) != null);
    T.check("getView set in onViewCreated", getView() == view);
  }

  @Override
  public void onStart() {
    super.onStart();
    mark("onStart");
  }

  @Override
  public void onResume() {
    super.onResume();
    mark("onResume");
  }

  @Override
  public void onPause() {
    mark("onPause");
    super.onPause();
  }

  @Override
  public void onStop() {
    mark("onStop");
    super.onStop();
  }

  @Override
  public void onSaveInstanceState(Bundle outState) {
    super.onSaveInstanceState(outState);
    mark("onSaveInstanceState");
    outState.putInt("counter", counter);
  }

  @Override
  public void onDestroyView() {
    mark("onDestroyView");
    T.check("view live in onDestroyView", getView() != null);
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
