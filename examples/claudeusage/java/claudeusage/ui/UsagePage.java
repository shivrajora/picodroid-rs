// SPDX-License-Identifier: GPL-3.0-only
package claudeusage.ui;

import claudeusage.R;
import claudeusage.data.UsageService;
import claudeusage.data.UsageSnapshot;
import picodroid.app.Fragment;
import picodroid.concurrent.Executors;
import picodroid.content.Context;
import picodroid.lifecycle.ViewModelProvider;
import picodroid.os.Bundle;
import picodroid.util.Log;
import picodroid.view.LayoutInflater;
import picodroid.view.View;
import picodroid.view.ViewGroup;
import picodroid.widget.FrameLayout;

/**
 * One screen's content, between the header and the footer: a {@link Fragment}, either a page of the
 * {@code ViewPager2} or the status screen laid over it.
 *
 * <p>A page is built a few views at a time: each view costs the RP2350 several milliseconds of LVGL
 * work, and a whole screen inside one UI tick would stall input and trip the slow-handler watchdog.
 * {@link #onCreateView} therefore returns the page's empty root at once, and {@link #onViewCreated}
 * starts a chain of main-thread posts that call {@link #buildNext} once per tick until it returns
 * false, with the page invisible meanwhile. The first paint follows, once the service has data, and
 * ends in the fade-in. Every post checks the fragment is still added with its view, the Android
 * idiom for work posted from a fragment; a page turn mid-build simply strands the chain.
 *
 * <p>The data comes from the Activity's {@link UsageViewModel}, which each page observes for as
 * long as its view lives. A page knows nothing else of its host, except that it may be a {@link
 * Host}.
 */
abstract class UsagePage extends Fragment {
  private static final String TAG = UsageService.TAG;

  /** What a page tells the Activity it is in, if that Activity cares to hear. */
  interface Host {
    /** A page finished building its views. */
    void onPageBuilt();
  }

  protected Context ctx;
  protected Palette palette;
  FrameLayout root;

  /** The header title, resolved once in {@link #onCreate}. */
  String title;

  protected int step;

  private Host host;
  private UsageViewModel model;
  private int fadeMs;
  private boolean built;
  private boolean painted;
  private boolean failed;
  private boolean awaitingData;

  /** Bumped whenever the view is created or destroyed: a post from an older view exits. */
  private int viewGen;

  private UsageSnapshot updatedSnapshot;
  private boolean updatedFresh;
  private long updatedMinute = -1;

  private boolean repaintPending;

  /** Posted once per publish while this page lives; allocated once rather than per second. */
  @SuppressWarnings("UnnecessaryLambda")
  private final Runnable repaint = this::repaint;

  /** The header title's string resource. */
  abstract int titleRes();

  /** Adds the next few views. Returns true while there is more to build. */
  abstract boolean buildNext();

  /** Repaint from the service. Called once built, then every second; must diff. */
  abstract void update(UsageService repo, long nowMs);

  /**
   * The first paint, one part per tick, while the page is still invisible: a page whose whole
   * {@link #update} overruns the RP2350's tick budget paints itself in parts here. Returns true
   * while there is more to paint. The default paints everything at once.
   */
  boolean paintNext(UsageService repo, long nowMs) {
    update(repo, nowMs);
    return false;
  }

  /** Whether the first paint waits for data from the bridge; the status screen's does not. */
  boolean needsData() {
    return true;
  }

  /** A build failed and the page is about to be built again from scratch: reset any counters. */
  void onBuildFailed() {}

  @Override
  public void onAttach(Context context) {
    super.onAttach(context);
    host = context instanceof Host ? (Host) context : null;
    ctx = context;
    palette = Palette.of(getResources());
  }

  @Override
  public void onCreate(Bundle savedInstanceState) {
    super.onCreate(savedInstanceState);
    title = getString(titleRes());
    fadeMs = getResources().getInteger(R.integer.fade_ms);
    model = new ViewModelProvider(requireActivity()).get(UsageViewModel.class);
  }

  @Override
  public View onCreateView(
      LayoutInflater inflater, ViewGroup container, Bundle savedInstanceState) {
    root = Ui.group(ctx, 0, 0, Ui.WIDTH, Ui.PAGE_HEIGHT, palette.background);
    root.setAlpha(0f);
    return root;
  }

  @Override
  public void onViewCreated(View view, Bundle savedInstanceState) {
    super.onViewCreated(view, savedInstanceState);
    model.usage().observe(getViewLifecycleOwner(), this::onUsage);
    restart();
  }

  @Override
  public void onDestroyView() {
    viewGen++;
    root = null;
    built = false;
    painted = false;
    super.onDestroyView();
  }

  @Override
  public void onDetach() {
    host = null;
    super.onDetach();
  }

  private void restart() {
    step = 0;
    repaintPending = false;
    built = false;
    painted = false;
    failed = false;
    awaitingData = false;
    updatedMinute = -1;
    final int gen = ++viewGen;
    Executors.mainExecutor().execute(() -> buildStep(gen));
  }

  private boolean gone(int gen) {
    // Detached with the Activity when that is destroyed, so isAdded() covers a dead host too.
    return gen != viewGen || !isAdded() || getView() == null;
  }

  private void buildStep(int gen) {
    if (gone(gen)) {
      return;
    }
    try {
      if (buildNext()) {
        Executors.mainExecutor().execute(() -> buildStep(gen));
        return;
      }
      built = true;
      Log.i(TAG, "built " + title);
      if (host != null) {
        host.onPageBuilt();
      }
      // The first paint takes its own ticks: with the last build step it overran the budget.
      Executors.mainExecutor().execute(() -> firstPaint(gen));
    } catch (OutOfMemoryError | RuntimeException e) {
      fail(e);
    }
  }

  /** The first paint, a part per tick while the page is invisible, then the fade-in. */
  private void firstPaint(int gen) {
    if (gone(gen) || !built || painted) {
      return;
    }
    UsageService repo = model.usage().getValue();
    if (repo == null || (needsData() && !model.hasData())) {
      awaitingData = true; // built behind the status screen; painted when the data arrives
      return;
    }
    awaitingData = false;
    try {
      if (paintNext(repo, System.currentTimeMillis())) {
        Executors.mainExecutor().execute(() -> firstPaint(gen));
        return;
      }
      painted = true;
      root.animate().alpha(1f).setDuration(fadeMs).start();
    } catch (OutOfMemoryError | RuntimeException e) {
      fail(e);
    }
  }

  /**
   * A half-built page would stay invisible for good. Drop what was built; the service's next tick
   * builds it again, by which time the collector has had a chance to run.
   */
  private void fail(Throwable e) {
    Log.w(TAG, "page build failed: " + e);
    failed = true;
    built = false;
    painted = false;
    if (root != null) {
      root.removeAllViews();
    }
    onBuildFailed();
  }

  /**
   * The observer: the service has something new, or a second passed. LiveData delivers inside the
   * Activity's own refresh, whose chrome repaint (~20 ms on the RP2350) and this page's (~30 ms)
   * would overrun the slow-handler budget together, so the page takes the next tick.
   */
  private void onUsage(UsageService repo) {
    if (repo == null || repaintPending) {
      return;
    }
    repaintPending = true;
    Executors.mainExecutor().execute(repaint);
  }

  /**
   * Paint the first time once built and the data is there, else repaint when the snapshot, the
   * freshness or the minute moved.
   */
  private void repaint() {
    repaintPending = false;
    UsageService repo = model.usage().getValue();
    if (repo == null || !isAdded() || getView() == null) {
      return;
    }
    long nowMs = System.currentTimeMillis();
    if (failed) {
      restart();
      return;
    }
    if (!painted) {
      if (awaitingData) {
        firstPaint(viewGen);
      }
      return;
    }
    UsageSnapshot s = repo.snapshot();
    boolean fresh = repo.isFresh();
    long minute = nowMs / 60_000L;
    if (s == updatedSnapshot && fresh == updatedFresh && minute == updatedMinute) {
      return; // everything on the data screens is minute-grained; nothing to repaint
    }
    updatedSnapshot = s;
    updatedFresh = fresh;
    updatedMinute = minute;
    update(repo, nowMs);
  }
}
