// SPDX-License-Identifier: GPL-3.0-only
package claudeusage.ui;

import claudeusage.R;
import claudeusage.data.UsageService;
import picodroid.app.Fragment;
import picodroid.content.Context;
import picodroid.lifecycle.ViewModelProvider;
import picodroid.os.Bundle;
import picodroid.util.Log;
import picodroid.view.LayoutInflater;
import picodroid.view.View;
import picodroid.view.ViewGroup;

/**
 * One screen's content, between the header and the footer: a {@link Fragment}, either a page of the
 * {@code ViewPager2} or the status screen laid over it.
 *
 * <p>A page is a layout of twenty to forty views, inflated whole in {@link #onCreateView}, as a
 * Fragment on Android inflates its layout; {@link #onViewCreated} binds the views, and the first
 * paint follows, once there is data, and ends in the fade-in. The main loop allows a screen build a
 * slow first frame (docs/designs/ui-build-budget-2026-10.md), and a page is well inside it.
 *
 * <p>What it paints is the Activity's {@link UsageViewModel} state, which each page observes for as
 * long as its view lives. A page knows nothing else of its host.
 */
abstract class UsagePage extends Fragment {
  private static final String TAG = UsageService.TAG;

  protected Context ctx;
  protected Palette palette;

  private UsageViewModel model;
  private boolean bound;
  private boolean painted;
  private boolean awaitingData;

  /** The header title's string resource. */
  abstract int titleRes();

  /** The page's layout. */
  abstract int layoutRes();

  /** Finds the views {@link #update} writes to, in the inflated {@link #layoutRes}. */
  abstract void onBind(View page);

  /** Paints {@code state}. Called once bound, then for every new state. */
  abstract void update(UsageUiState state, long nowMs);

  /** Whether the page shows the bridge's numbers and so waits for them; the status screen not. */
  boolean needsData() {
    return true;
  }

  @Override
  public void onAttach(Context context) {
    super.onAttach(context);
    ctx = context;
    palette = Palette.of(getResources());
  }

  @Override
  public void onCreate(Bundle savedInstanceState) {
    super.onCreate(savedInstanceState);
    model = new ViewModelProvider(requireActivity()).get(UsageViewModel.class);
  }

  @Override
  public View onCreateView(
      LayoutInflater inflater, ViewGroup container, Bundle savedInstanceState) {
    return inflater.inflate(layoutRes(), container, false);
  }

  /** The views are there: bind them, observe the state, and paint when there is data. */
  @Override
  public void onViewCreated(View view, Bundle savedInstanceState) {
    super.onViewCreated(view, savedInstanceState);
    painted = false;
    awaitingData = false;
    view.setAlpha(0f); // out of sight until the first paint, then the fade-in
    onBind(view);
    bound = true;
    Log.i(TAG, "built " + getString(titleRes()));
    model.state().observe(getViewLifecycleOwner(), this::onState);
    firstPaint(view);
  }

  @Override
  public void onDestroyView() {
    bound = false;
    painted = false;
    super.onDestroyView();
  }

  /** Whether {@code host} is no longer this fragment's view: a post made for it has no work. */
  private boolean gone(View host) {
    // Detached with the Activity when that is destroyed, so isAdded() covers a dead host too.
    return !isAdded() || getView() != host;
  }

  /** The state this page can paint now, or null: a data page has nothing to paint before data. */
  private UsageUiState paintable() {
    UsageUiState state = model.state().getValue();
    return state == null || (needsData() && !state.hasData()) ? null : state;
  }

  /** The first paint, while the page is still invisible, then the fade-in. */
  private void firstPaint(View host) {
    if (gone(host) || !bound || painted) {
      return;
    }
    UsageUiState state = paintable();
    if (state == null) {
      awaitingData = true; // built behind the status screen; painted when the data arrives
      return;
    }
    awaitingData = false;
    update(state, System.currentTimeMillis());
    painted = true;
    host.animate().alpha(1f).setDuration(getResources().getInteger(R.integer.fade_ms)).start();
  }

  /**
   * The observer: something on screen changed. A painted page repaints in the call, beside the
   * chrome, as an Android observer would; a page built before the data came takes its first paint
   * here.
   */
  private void onState(UsageUiState state) {
    View host = getView();
    if (!isAdded() || host == null) {
      return;
    }
    if (painted) {
      UsageUiState paintable = paintable();
      if (paintable != null) {
        update(paintable, System.currentTimeMillis());
      }
    } else if (awaitingData) {
      awaitingData = false;
      firstPaint(host);
    }
  }
}
