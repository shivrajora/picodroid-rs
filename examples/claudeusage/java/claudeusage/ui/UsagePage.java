// SPDX-License-Identifier: GPL-3.0-only
package claudeusage.ui;

import claudeusage.R;
import claudeusage.data.UsageService;
import picodroid.app.Fragment;
import picodroid.concurrent.Executor;
import picodroid.content.Context;
import picodroid.lifecycle.ViewModelProvider;
import picodroid.os.Bundle;
import picodroid.util.Log;
import picodroid.view.AsyncLayoutInflater;
import picodroid.view.LayoutInflater;
import picodroid.view.View;
import picodroid.view.ViewGroup;

/**
 * One screen's content, between the header and the footer: a {@link Fragment}, either a page of the
 * {@code ViewPager2} or the status screen laid over it.
 *
 * <p>A page is a layout of twenty to forty views, and each view costs the RP2350 milliseconds: a
 * whole screen inflated in one UI tick would stall input and trip the slow-handler watchdog. So
 * {@link #onCreateView} returns an empty, transparent host at once, and {@link #onViewCreated}
 * hands the page's own layout to an {@link AsyncLayoutInflater}, which builds it over the following
 * ticks. Its callback binds the views; the first paint follows, once there is data, and ends in the
 * fade-in.
 *
 * <p>What it paints is the Activity's {@link UsageViewModel} state, which each page observes for as
 * long as its view lives. A page knows nothing else of its host.
 */
abstract class UsagePage extends Fragment {
  private static final String TAG = UsageService.TAG;

  protected Context ctx;
  protected Palette palette;

  private Executor main;
  private UsageViewModel model;
  private boolean bound;
  private boolean painted;
  private boolean awaitingData;

  /** The header title's string resource. */
  abstract int titleRes();

  /** The page's layout, inflated into the host. */
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
    main = context.getMainExecutor();
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
    return inflater.inflate(R.layout.page_host, container, false);
  }

  @Override
  public void onViewCreated(View view, Bundle savedInstanceState) {
    super.onViewCreated(view, savedInstanceState);
    bound = false;
    painted = false;
    awaitingData = false;
    model.state().observe(getViewLifecycleOwner(), this::onState);
    final ViewGroup host = (ViewGroup) view;
    new AsyncLayoutInflater(ctx)
        .inflate(layoutRes(), host, (page, resid, parent) -> onInflated(host, page));
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

  /** The page's layout is whole: into the host with it, unless the page was turned meanwhile. */
  private void onInflated(ViewGroup host, View page) {
    if (gone(host)) {
      page.close(); // never added anywhere: picodroid frees a view's widget only when told
      return;
    }
    host.addView(page);
    onBind(page);
    bound = true;
    Log.i(TAG, "built " + getString(titleRes()));
    firstPaint(host);
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
   * chrome: measured together on the RP2350 they stay inside the tick budget. The first paint of a
   * page built before the data came does not: its call sites are cold, and with the chrome's
   * repaint it overran the budget by a sixth, so that one takes the next tick.
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
      main.execute(() -> firstPaint(host));
    }
  }
}
