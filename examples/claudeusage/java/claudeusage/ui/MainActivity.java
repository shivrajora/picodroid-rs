// SPDX-License-Identifier: GPL-3.0-only
package claudeusage.ui;

import claudeusage.R;
import claudeusage.data.LinkState;
import claudeusage.data.UsageService;
import claudeusage.data.UsageSnapshot;
import claudeusage.hardware.RgbLed;
import claudeusage.util.TimeFormat;
import picodroid.app.Activity;
import picodroid.app.Fragment;
import picodroid.app.FragmentManager;
import picodroid.concurrent.Executors;
import picodroid.concurrent.ScheduledExecutorService;
import picodroid.concurrent.ScheduledFuture;
import picodroid.concurrent.TimeUnit;
import picodroid.content.Context;
import picodroid.content.Intent;
import picodroid.content.res.Resources;
import picodroid.lifecycle.ViewModel;
import picodroid.lifecycle.ViewModelProvider;
import picodroid.os.Bundle;
import picodroid.util.AttributeSet;
import picodroid.util.Log;
import picodroid.view.KeyEvent;
import picodroid.view.View;
import picodroid.widget.TextView;
import picodroid.widget.ViewPager2;

/**
 * The one Activity, declared in the manifest as the entry point. Four screens live in it as
 * Fragments in a {@link ViewPager2} rather than as Activities of their own: one key handler, one
 * header and footer, and a page switch is a fragment swap in the pager rather than a lifecycle of
 * the Activity's own. The status screen, shown while there is no data, is a Fragment laid over the
 * pager in the same container. The chrome is {@code res/layout/activity_main.xml}.
 *
 * <p>The numbers come from {@link UsageService}, started here so they stay warm, by way of the
 * {@link UsageViewModel}: the Activity observes its state for the chrome, the pages observe it for
 * themselves, and the Activity never looks for the page on screen. Buttons, with the display
 * landscape (A top-left, B bottom-left, X top-right, Y bottom-right); each corner of the screen
 * carries the hint for the button beside it:
 *
 * <ul>
 *   <li>A previous screen, B next screen (both wrap); hold either to keep turning
 *   <li>X sync now; hold X to look for the bridge on the LAN again
 *   <li>Y home (Limits); hold Y to toggle AUTO, which cycles the screens
 * </ul>
 *
 * Four buttons, eight actions, the way Android gives a key two: A and B act on the press and keep
 * acting on its auto-repeats; X and Y only start tracking on the press, run the long action from
 * {@link #onKeyLongPress} (which cancels the release) and the short action from {@link #onKeyUp}. Y
 * never leaves the app: this is an appliance, and BACK falling through to finish() would drop it to
 * a launcher nobody asked for.
 */
public class MainActivity extends Activity {
  private static final String TAG = UsageService.TAG;

  private static final int PAGE_LIMITS = 0;
  private static final int PAGE_COUNT = UsagePagerAdapter.COUNT;

  /**
   * A held A or B turns a page every this many auto-repeats: the first turn at the long-press
   * (repeat 1, 400 ms), then one about every half second at the 50 ms repeat delay.
   */
  private static final int PAGE_TURN_REPEATS = 8;

  private static final String STATE_PAGER = "pager";
  private static final String STATE_AUTO = "auto";

  /** AUTO is a setting: it survives a power cycle. */
  private static final String KEY_AUTO = "auto";

  /** The status screen's tag in the fragment manager. */
  private static final String TAG_STATUS = "status";

  private static final int[] PAGE_DOT_IDS = {
    R.id.page_dot_0, R.id.page_dot_1, R.id.page_dot_2, R.id.page_dot_3
  };

  /** A page turned: the title, the hint and the dot follow; AUTO's countdown restarts. */
  private final ViewPager2.OnPageChangeCallback pageCallback =
      new ViewPager2.OnPageChangeCallback() {
        @Override
        public void onPageSelected(int position) {
          restartAuto();
          if (!statusShowing) {
            Log.i(TAG, "page -> " + pageTitles[position]);
          }
          refreshPageChrome();
        }
      };

  /** AUTO's page turns; a task on the main thread, running only while the Activity is started. */
  private final ScheduledExecutorService timer = Executors.mainScheduledExecutor();

  private ScheduledFuture<?> autoTurn;

  private UsageViewModel model;
  private Palette palette;
  private int autoPeriod;
  private RgbLed led;

  private ViewPager2 pager;
  private TextView title;
  private TextView plan;
  private TextView clock;
  private TextView syncHint;
  private TextView homeHint;
  private TextView banner;
  private View statusDot;
  private final View[] pageDots = new View[PAGE_COUNT];
  private final String[] pageTitles = new String[PAGE_COUNT];
  private String statusTitle;
  private String hintAuto;
  private String hintHome;
  private String planText = "";
  private UsageSnapshot planOf;

  /** Whether the status screen is laid over the pager (no data yet). */
  private boolean statusShowing;

  private boolean auto;
  private boolean started;

  /** What the chrome last painted; null until the ViewModel has published. */
  private UsageUiState state;

  @Override
  protected void onCreate(Bundle savedInstanceState) {
    super.onCreate(savedInstanceState);
    setTheme(R.style.AppTheme);
    Resources res = getResources();
    palette = Palette.of(res);
    model = new ViewModelProvider(this).get(UsageViewModel.class);
    autoPeriod = res.getInteger(R.integer.auto_seconds);
    hintAuto = getString(R.string.hint_auto);
    hintHome = getString(R.string.hint_home);
    for (int i = 0; i < PAGE_COUNT; i++) {
      pageTitles[i] = getString(UsagePagerAdapter.titleRes(i));
    }
    statusTitle = getString(R.string.page_status);
    led = RgbLed.open();

    setContentView(R.layout.activity_main);
    bindChrome();

    if (savedInstanceState != null) {
      pager.restoreState(savedInstanceState.getBundle(STATE_PAGER));
      auto = savedInstanceState.getBoolean(STATE_AUTO, false);
    } else {
      auto = getSharedPreferences(UsageService.PREFS, MODE_PRIVATE).getBoolean(KEY_AUTO, false);
    }
    // The first page is inflated behind the status screen while the board waits for WiFi and the
    // first fetch, and it paints the moment the data arrives.
    pager.setAdapter(new UsagePagerAdapter(this));

    startService(new Intent(this, UsageService.class));
    model.state().observe(this, this::onState);
    Log.i(TAG, "ui ready");
  }

  /** Finds the chrome. */
  private void bindChrome() {
    pager = findViewById(R.id.pager);
    pager.setUserInputEnabled(false); // four buttons and no touch panel: keys turn the pages
    pager.registerOnPageChangeCallback(pageCallback);

    title = findViewById(R.id.title);
    plan = findViewById(R.id.plan);
    clock = findViewById(R.id.clock);
    syncHint = findViewById(R.id.hint_sync);
    homeHint = findViewById(R.id.hint_home);
    banner = findViewById(R.id.banner);

    statusDot = findViewById(R.id.status_dot);
    for (int i = 0; i < PAGE_COUNT; i++) {
      pageDots[i] = findViewById(PAGE_DOT_IDS[i]);
    }
  }

  /**
   * The views of the app's own that the page layouts name: there is no reflection to construct them
   * by, so the Activity, which is every layout's factory, does.
   */
  @Override
  public View onCreateView(String name, Context context, AttributeSet attrs) {
    if (name.equals("claudeusage.ui.RingView")) {
      return new RingView(context, attrs);
    }
    if (name.equals("claudeusage.ui.TrendChart")) {
      return new TrendChart(context, attrs);
    }
    if (name.equals("claudeusage.ui.WeekChart")) {
      return new WeekChart(context, attrs);
    }
    return super.onCreateView(name, context, attrs);
  }

  /** There is no reflection to make a ViewModel by: the pages' provider asks here. */
  @Override
  public ViewModelProvider.Factory getDefaultViewModelProviderFactory() {
    return new ViewModelProvider.Factory() {
      @Override
      @SuppressWarnings("unchecked")
      public <T extends ViewModel> T create(Class<T> modelClass) {
        if (modelClass != UsageViewModel.class) {
          throw new IllegalArgumentException("Unknown ViewModel class " + modelClass.getName());
        }
        return (T) new UsageViewModel();
      }
    };
  }

  // ── Lifecycle ──────────────────────────────────────────────────────────────

  @Override
  public void onStart() {
    super.onStart();
    started = true;
    restartAuto();
  }

  @Override
  public void onStop() {
    started = false;
    restartAuto();
    super.onStop();
  }

  @Override
  public void onDestroy() {
    timer.shutdownNow();
    led.close();
    super.onDestroy();
  }

  @Override
  protected void onSaveInstanceState(Bundle outState) {
    super.onSaveInstanceState(outState);
    outState.putBundle(STATE_PAGER, pager.saveState());
    outState.putBoolean(STATE_AUTO, auto);
  }

  // ── Input ──────────────────────────────────────────────────────────────────

  /**
   * The four keys are consumed here, BACK included: an appliance never finishes to the launcher,
   * and consuming BACK's press (without calling super) is what keeps the default {@code onKeyUp}
   * from running {@code onBackPressed}, as on Android. A and B act here, on the press and again on
   * every {@link #PAGE_TURN_REPEATS}th auto-repeat while held; X and Y only start tracking, so
   * {@link #onKeyLongPress} and {@link #onKeyUp} can tell a hold from a press.
   */
  @Override
  public boolean onKeyDown(int code, KeyEvent event) {
    int repeat = event.getRepeatCount();
    switch (code) {
      case KeyEvent.KEYCODE_DPAD_UP:
      case KeyEvent.KEYCODE_DPAD_DOWN:
        if (statusShowing) {
          return true; // nothing to page through yet
        }
        if (repeat == 0 || (repeat - 1) % PAGE_TURN_REPEATS == 0) {
          turnPage(code == KeyEvent.KEYCODE_DPAD_UP ? -1 : 1);
        }
        return true;
      case KeyEvent.KEYCODE_DPAD_CENTER:
      case KeyEvent.KEYCODE_BACK:
        if (repeat == 0) {
          event.startTracking();
        }
        return true;
      default:
        return super.onKeyDown(code, event);
    }
  }

  /** The long actions: X held looks for the bridge again, Y held toggles AUTO from any screen. */
  @Override
  public boolean onKeyLongPress(int code, KeyEvent event) {
    switch (code) {
      case KeyEvent.KEYCODE_DPAD_CENTER:
        Log.i(TAG, "rediscover requested");
        model.rediscover();
        return true;
      case KeyEvent.KEYCODE_BACK:
        toggleAuto();
        return true;
      default:
        return super.onKeyLongPress(code, event);
    }
  }

  /**
   * The short actions, on a release whose press was tracked here and whose long action did not run:
   * X syncs now, Y goes home to Limits.
   */
  @Override
  public boolean onKeyUp(int code, KeyEvent event) {
    switch (code) {
      case KeyEvent.KEYCODE_DPAD_CENTER:
        if (event.isTracking() && !event.isCanceled()) {
          Log.i(TAG, "sync requested");
          model.refreshNow();
        }
        return true;
      case KeyEvent.KEYCODE_BACK:
        if (event.isTracking()
            && !event.isCanceled()
            && !statusShowing
            && pager.getCurrentItem() != PAGE_LIMITS) {
          pager.setCurrentItem(PAGE_LIMITS, false);
        }
        return true;
      case KeyEvent.KEYCODE_DPAD_UP:
      case KeyEvent.KEYCODE_DPAD_DOWN:
        return true;
      default:
        return super.onKeyUp(code, event);
    }
  }

  private void toggleAuto() {
    auto = !auto;
    Log.i(TAG, auto ? "auto on" : "auto off");
    getSharedPreferences(UsageService.PREFS, MODE_PRIVATE)
        .edit()
        .putBoolean(KEY_AUTO, auto)
        .apply();
    restartAuto();
    if (state != null) {
      refreshChrome(state);
    }
  }

  /**
   * Starts AUTO's countdown again from the top, or stops it: on a toggle, on any page change (a
   * turn by hand restarts it, as going home does) and as the Activity starts and stops.
   */
  private void restartAuto() {
    if (autoTurn != null) {
      autoTurn.cancel(false);
      autoTurn = null;
    }
    if (auto && started) {
      autoTurn =
          timer.scheduleWithFixedDelay(this::autoTurn, autoPeriod, autoPeriod, TimeUnit.SECONDS);
    }
  }

  /** AUTO's period is up: the next screen, while there are live numbers to cycle through. */
  private void autoTurn() {
    if (!statusShowing && state != null && state.fresh) {
      turnPage(1);
    }
  }

  /**
   * Turns the pager by {@code by} pages, wrapping (the pager itself does not). The pager replaces
   * the page over its own ticks and the page fades itself in once painted, so the turn asks for no
   * scroll animation.
   */
  private void turnPage(int by) {
    pager.setCurrentItem((pager.getCurrentItem() + by + PAGE_COUNT) % PAGE_COUNT, false);
  }

  // ── What the ViewModel publishes (main thread) ─────────────────────────────

  /**
   * A new state: the status screen comes or goes, and the chrome repaints. The pages observe the
   * same state and repaint on a tick of their own, after this one: the chrome costs the RP2350 some
   * 20 ms, and a page's repaint on top would overrun the budget.
   */
  private void onState(UsageUiState newState) {
    state = newState;
    syncStatus(newState);
    refreshChrome(newState);
  }

  /**
   * Lays the status screen over the pager while there has never been any data, and takes it away
   * once there is: a fragment {@code replace} into the pager's container.
   */
  private void syncStatus(UsageUiState s) {
    boolean wantStatus = !s.hasData();
    if (wantStatus == statusShowing) {
      return;
    }
    statusShowing = wantStatus;
    FragmentManager fm = getSupportFragmentManager();
    if (wantStatus) {
      fm.beginTransaction().replace(R.id.page_host, new StatusPage(), TAG_STATUS).commit();
      Log.i(TAG, "page -> " + statusTitle);
    } else {
      Fragment status = fm.findFragmentByTag(TAG_STATUS);
      if (status != null) {
        fm.beginTransaction().remove(status).commit();
      }
      Log.i(TAG, "page -> " + pageTitles[pager.getCurrentItem()]);
    }
  }

  /** The part of the chrome a page turn changes; cheap enough to share a tick with the turn. */
  private void refreshPageChrome() {
    int current = pager.getCurrentItem();
    title.setText(statusShowing ? statusTitle : pageTitles[current]);
    homeHint.setText(current == PAGE_LIMITS || statusShowing ? hintAuto : hintHome);
    homeHint.setTextColor(auto && current == PAGE_LIMITS ? palette.clay : palette.faint);
    int active = statusShowing ? -1 : current;
    for (int i = 0; i < PAGE_COUNT; i++) {
      Ui.tint(pageDots[i], i == active ? palette.clay : palette.track);
    }
  }

  /** Header, footer and LED: everything outside the page. */
  private void refreshChrome(UsageUiState s) {
    UsageSnapshot snapshot = s.snapshot;
    refreshPageChrome();
    if (snapshot != planOf) {
      planOf = snapshot; // one upper-casing per snapshot, not per refresh
      planText = snapshot == null ? "" : snapshot.plan.toUpperCase();
    }
    plan.setText(statusShowing ? "" : planText);
    clock.setText(snapshot != null ? TimeFormat.hm(s.minute * 60_000L) : "");
    syncHint.setTextColor(s.syncing ? palette.clay : palette.faint);

    // Green: live. Amber: live numbers, but the last attempt failed. Red: not live.
    int dot = s.fresh ? (s.link == LinkState.OK ? palette.good : palette.warn) : palette.bad;
    if (s.link == LinkState.JOINING) {
      dot = palette.clay;
    }
    Ui.tint(statusDot, dot);

    if (statusShowing) {
      banner.setText("");
    } else if (s.fresh && s.link == LinkState.OK) {
      // AUTO is toggled from Limits only, so say it is on wherever the cycle has got to.
      banner.setText(
          getString(
              auto ? R.string.banner_auto : R.string.banner_updated,
              TimeFormat.hm(s.lastGoodWallMs)));
      banner.setTextColor(palette.muted);
    } else {
      String what = getString(s.link.shortText(s.linkErr));
      // While still fresh this is one missed poll: say so quietly. Once stale, say it in clay.
      banner.setText(
          s.staleMinutes >= 1
              ? getString(
                  R.string.banner_stale, what, TimeFormat.duration(s.staleMinutes * 60_000L))
              : what);
      banner.setTextColor(s.fresh ? palette.muted : palette.clay);
    }

    int ledColor = 0;
    if (s.fresh && snapshot != null) {
      int worst = Math.max(snapshot.sessionPct, snapshot.weeklyPct);
      ledColor =
          worst >= palette.badFrom
              ? palette.ledBad
              : (worst >= palette.warnFrom ? palette.ledWarn : 0);
    }
    led.setColor(ledColor);
  }
}
