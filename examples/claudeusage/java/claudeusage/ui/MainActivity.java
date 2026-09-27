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
import picodroid.content.Intent;
import picodroid.content.ServiceConnection;
import picodroid.content.res.Resources;
import picodroid.os.Bundle;
import picodroid.os.IBinder;
import picodroid.util.Log;
import picodroid.view.KeyEvent;
import picodroid.widget.FrameLayout;
import picodroid.widget.LinearLayout;
import picodroid.widget.ViewPager2;

/**
 * The one Activity, declared in the manifest as the entry point. Four screens live in it as
 * Fragments in a {@link ViewPager2} rather than as Activities of their own: one key handler, one
 * header and footer, and a page switch is a fragment swap in the pager rather than a lifecycle of
 * the Activity's own. The status screen, shown while there is no data, is a Fragment laid over the
 * pager in the same container. The chrome is {@code res/layout/activity_main.xml}.
 *
 * <p>The numbers come from {@link UsageService}, started here so they stay warm and bound while the
 * screen is on. Buttons, with the display landscape (A top-left, B bottom-left, X top-right, Y
 * bottom-right); each corner of the screen carries the hint for the button beside it:
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
public class MainActivity extends Activity implements UsageService.Listener {
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

  /** The status screen's tag in the fragment manager; the pager's pages are {@code f<index>}. */
  private static final String TAG_STATUS = "status";

  private static final int[] PAGE_DOT_IDS = {
    R.id.page_dot_0, R.id.page_dot_1, R.id.page_dot_2, R.id.page_dot_3
  };

  private final ServiceConnection connection =
      new ServiceConnection() {
        @Override
        public void onServiceConnected(IBinder binder) {
          repo = ((UsageService.LocalBinder) binder).service;
          // The first chrome paint costs the RP2350 some 20 ms and the listener's first tick
          // thread a few more: each takes a tick of its own rather than the connect callback's.
          Executors.mainExecutor().execute(MainActivity.this::onConnected);
        }

        @Override
        public void onServiceDisconnected() {
          repo = null;
        }
      };

  /** A page turned: the title, the hint and the dot follow; AUTO's countdown restarts. */
  private final ViewPager2.OnPageChangeCallback pageCallback =
      new ViewPager2.OnPageChangeCallback() {
        @Override
        public void onPageSelected(int position) {
          autoSeconds = 0;
          if (!statusShowing) {
            Log.i(TAG, "page -> " + pageTitles[position]);
          }
          refreshPageChrome();
        }
      };

  /** Null until the Service is bound; every callback that reads it arrives after that. */
  private UsageService repo;

  private Palette palette;
  private int autoPeriod;
  private int fadeMs;
  private RgbLed led;

  private ViewPager2 pager;
  private UsagePagerAdapter adapter;
  private Line title;
  private Line plan;
  private Line clock;
  private Line syncHint;
  private Line homeHint;
  private Line banner;
  private FrameLayout statusDot;
  private final FrameLayout[] pageDots = new FrameLayout[PAGE_COUNT];
  private final String[] pageTitles = new String[PAGE_COUNT];
  private String statusTitle;
  private int shownStatusColor;
  private int shownPageDot = -1;
  private String hintAuto;
  private String hintSync;
  private String planText = "";
  private UsageSnapshot planOf;
  private String hintHome;
  private String bannerAuto;
  private String bannerUpdated;
  private String bannerStale;

  /** Whether the status screen is laid over the pager (no data yet). */
  private boolean statusShowing;

  private boolean pageUpdatePending;
  private boolean dotsPending;
  private boolean chromeWarmed;
  private boolean auto;
  private int autoSeconds;
  private boolean destroyed;
  private long tickMinute = -1;
  private LinkState tickState;
  private boolean tickFresh;

  @Override
  protected void onCreate(Bundle savedInstanceState) {
    super.onCreate(savedInstanceState);
    Resources res = getResources();
    palette = new Palette(res);
    palette.applyTheme();
    autoPeriod = res.getInteger(R.integer.auto_seconds);
    fadeMs = res.getInteger(R.integer.fade_ms);
    hintAuto = getString(R.string.hint_auto);
    hintHome = getString(R.string.hint_home);
    hintSync = getString(R.string.hint_sync);
    bannerAuto = getString(R.string.banner_auto);
    bannerUpdated = getString(R.string.banner_updated);
    bannerStale = getString(R.string.banner_stale);
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
    // The first page builds behind the status screen while the board waits for WiFi and the
    // first fetch: its one-off costs (loading the page's classes, the cold call sites) are paid in
    // that idle time, and it paints the moment the data arrives.
    adapter = new UsagePagerAdapter(this);
    pager.setAdapter(adapter);

    startService(new Intent(UsageService.class));
    Log.i(TAG, "ui ready");
  }

  /** Finds the chrome. */
  private void bindChrome() {
    pager = findViewById(R.id.pager);
    pager.setUserInputEnabled(false); // four buttons and no touch panel: keys turn the pages
    pager.registerOnPageChangeCallback(pageCallback);

    title = new Line(findViewById(R.id.title), palette.text);
    plan = new Line(findViewById(R.id.plan), palette.clay);
    clock = new Line(findViewById(R.id.clock), palette.muted);
    syncHint = new Line(findViewById(R.id.hint_sync), hintSync, palette.faint);
    homeHint = new Line(findViewById(R.id.hint_home), hintAuto, palette.faint);
    banner = new Line(findViewById(R.id.banner), palette.muted);

    statusDot = findViewById(R.id.status_dot);
    shownStatusColor = palette.faint;
    Ui.fill(statusDot, palette.faint, 4);
    LinearLayout dots = findViewById(R.id.page_dots);
    dots.setSpacing(getResources().getDimensionPixelSize(R.dimen.page_dot_gap));
    for (int i = 0; i < PAGE_COUNT; i++) {
      pageDots[i] = findViewById(PAGE_DOT_IDS[i]);
      Ui.fill(pageDots[i], palette.track, 3);
    }
  }

  // ── What the pages ask of their host ───────────────────────────────────

  Palette palette() {
    return palette;
  }

  int fadeMs() {
    return fadeMs;
  }

  UsageService repo() {
    return repo;
  }

  boolean destroyed() {
    return destroyed;
  }

  boolean hasData() {
    UsageSnapshot s = repo == null ? null : repo.snapshot();
    return s != null && s.hasLimits();
  }

  /** The first page is built: warm the chrome's data path while the board is still idle. */
  void onPageBuilt() {
    if (!chromeWarmed) {
      chromeWarmed = true;
      Executors.mainExecutor().execute(this::warmChrome);
    }
  }

  // ── Lifecycle ──────────────────────────────────────────────────────────────

  private void onConnected() {
    if (destroyed || repo == null) {
      return;
    }
    repo.setListener(this);
    refresh();
  }

  @Override
  public void onStart() {
    super.onStart();
    bindService(new Intent(UsageService.class), connection);
  }

  @Override
  public void onResume() {
    super.onResume();
    if (repo != null) {
      repo.setListener(this);
      refresh();
    }
  }

  @Override
  public void onPause() {
    if (repo != null) {
      repo.setListener(null);
    }
    super.onPause();
  }

  @Override
  public void onStop() {
    if (repo != null) {
      repo.setListener(null);
      repo = null;
    }
    unbindService(connection);
    super.onStop();
  }

  @Override
  public void onDestroy() {
    destroyed = true;
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
   * Every key is consumed here, BACK included: an appliance never finishes to the launcher, and
   * consuming BACK's press (without calling super) is what keeps the default {@code onKeyUp} from
   * running {@code onBackPressed}, as on Android. A and B act here, on the press and again on every
   * {@link #PAGE_TURN_REPEATS}th auto-repeat while held; X and Y only start tracking, so {@link
   * #onKeyLongPress} and {@link #onKeyUp} can tell a hold from a press.
   */
  @Override
  public boolean onKeyDown(int code, KeyEvent event) {
    if (repo == null) {
      return true;
    }
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
        return true;
    }
  }

  /** The long actions: X held looks for the bridge again, Y held toggles AUTO from any screen. */
  @Override
  public boolean onKeyLongPress(int code, KeyEvent event) {
    if (repo == null) {
      return true;
    }
    switch (code) {
      case KeyEvent.KEYCODE_DPAD_CENTER:
        Log.i(TAG, "rediscover requested");
        repo.rediscover();
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
    if (repo == null || !event.isTracking() || event.isCanceled()) {
      return true;
    }
    switch (code) {
      case KeyEvent.KEYCODE_DPAD_CENTER:
        Log.i(TAG, "sync requested");
        repo.refreshNow();
        return true;
      case KeyEvent.KEYCODE_BACK:
        if (!statusShowing && pager.getCurrentItem() != PAGE_LIMITS) {
          autoSeconds = 0; // home restarts the AUTO countdown, as a turn does
          pager.setCurrentItem(PAGE_LIMITS, false);
        }
        return true;
      default:
        return true;
    }
  }

  private void toggleAuto() {
    auto = !auto;
    autoSeconds = 0;
    Log.i(TAG, auto ? "auto on" : "auto off");
    saveAuto(auto);
    refreshChrome();
  }

  /**
   * Off the main thread: the SDK's {@code apply()} writes LittleFS synchronously, and that write
   * alone overran the slow-handler budget in the sim.
   */
  private void saveAuto(final boolean on) {
    Executors.backgroundExecutor()
        .execute(
            () ->
                getSharedPreferences(UsageService.PREFS, MODE_PRIVATE)
                    .edit()
                    .putBoolean(KEY_AUTO, on)
                    .apply());
  }

  /**
   * Turns the pager by {@code by} pages, wrapping (the pager itself does not). The pager replaces
   * the page over its own ticks and the page fades itself in once painted, so the turn asks for no
   * scroll animation.
   */
  private void turnPage(int by) {
    autoSeconds = 0;
    pager.setCurrentItem((pager.getCurrentItem() + by + PAGE_COUNT) % PAGE_COUNT, false);
  }

  // ── Pages ──────────────────────────────────────────────────────────────────

  /**
   * Lays the status screen over the pager while there has never been any data, and takes it away
   * once there is: a fragment {@code replace} into the pager's container, committed on a tick of
   * its own as the old page swap was.
   */
  private void syncStatus() {
    boolean wantStatus = !hasData();
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
    refreshPageChrome();
  }

  /** The screen the user sees: the status screen, or the pager's current page; null mid-swap. */
  private UsagePage visiblePage() {
    FragmentManager fm = getSupportFragmentManager();
    Fragment f = fm.findFragmentByTag(statusShowing ? TAG_STATUS : "f" + pager.getCurrentItem());
    return (UsagePage) f;
  }

  /**
   * Runs the chrome's data-path formatting once, on placeholder values, while the board is still
   * idle. The runtime resolves a call site once per app run, keyed by the caller's constant pool,
   * so the first refresh with real data then finds these sites resolved and their classes
   * initialised: on the RP2350 that refresh was the last slow tick from power-on (57 ms, of which
   * 89 cold resolutions and the class initialisations were a third).
   */
  private void warmChrome() {
    if (destroyed) {
      return;
    }
    String at = TimeFormat.hm(System.currentTimeMillis());
    String warmed =
        String.format(bannerUpdated, at)
            + String.format(bannerAuto, at)
            + String.format(bannerStale, hintHome, TimeFormat.duration(60_000L))
            + hintSync.toUpperCase();
    Log.i(TAG, "chrome warm, " + warmed.length() + " chars");
  }

  // ── Service callbacks (main thread) ────────────────────────────────────────

  @Override
  public void onUsageChanged() {
    refresh();
  }

  @Override
  public void onTick() {
    if (repo == null) {
      return;
    }
    if (auto && !statusShowing && repo.isFresh() && ++autoSeconds >= autoPeriod) {
      turnPage(1);
      return;
    }
    // A full repaint builds a dozen strings just to find that none changed, which costs the
    // RP2350 some 55 ms: over the slow-handler budget, every second. Everything on the data
    // screens is minute-grained, so repaint only when the minute, the link or the freshness moved.
    long minute = System.currentTimeMillis() / 60_000L;
    LinkState state = repo.linkState();
    boolean fresh = repo.isFresh();
    if (statusShowing || minute != tickMinute || state != tickState || fresh != tickFresh) {
      tickMinute = minute;
      tickState = state;
      tickFresh = fresh;
      refresh();
    }
  }

  private void refresh() {
    if (destroyed || repo == null) {
      return;
    }
    syncStatus();
    refreshChrome();
    if (!pageUpdatePending) {
      // The chrome (~20 ms on the RP2350) and the page (~30 ms) would overrun the slow-handler
      // budget together, so the page takes the next tick.
      pageUpdatePending = true;
      Executors.mainExecutor().execute(this::updatePage);
    }
  }

  private void updatePage() {
    pageUpdatePending = false;
    if (destroyed || repo == null) {
      return;
    }
    UsagePage page = visiblePage();
    if (page != null) {
      page.onUsage(repo, System.currentTimeMillis());
    }
  }

  /** The part of the chrome a page turn changes; cheap enough to share a tick with the turn. */
  private void refreshPageChrome() {
    int current = pager.getCurrentItem();
    title.show(statusShowing ? statusTitle : pageTitles[current], palette.text);
    homeHint.show(
        current == PAGE_LIMITS || statusShowing ? hintAuto : hintHome,
        auto && current == PAGE_LIMITS ? palette.clay : palette.faint);
    int activeDot = statusShowing ? -1 : current;
    if (activeDot != shownPageDot && !dotsPending) {
      // Each fill costs the RP2350 some 4 ms inside the flex row, so the dots take their own tick.
      dotsPending = true;
      Executors.mainExecutor().execute(this::movePageDot);
    }
  }

  private void movePageDot() {
    dotsPending = false;
    if (destroyed) {
      return;
    }
    int activeDot = statusShowing ? -1 : pager.getCurrentItem();
    if (activeDot == shownPageDot) {
      return;
    }
    if (shownPageDot >= 0) {
      Ui.fill(pageDots[shownPageDot], palette.track, 3);
    }
    if (activeDot >= 0) {
      Ui.fill(pageDots[activeDot], palette.clay, 3);
    }
    shownPageDot = activeDot;
  }

  /** Header, footer and LED: everything outside the page. */
  private void refreshChrome() {
    UsageSnapshot s = repo.snapshot();
    boolean fresh = repo.isFresh();
    LinkState state = repo.linkState();
    long now = System.currentTimeMillis();

    refreshPageChrome();
    if (s != planOf) {
      planOf = s; // one upper-casing per snapshot, not per refresh
      planText = s == null ? "" : s.plan.toUpperCase();
    }
    plan.show(!statusShowing ? planText : "", palette.clay);
    clock.show(s != null ? TimeFormat.hm(now) : "", palette.muted);
    syncHint.show(hintSync, repo.isSyncing() ? palette.clay : palette.faint);

    // Green: live. Amber: live numbers, but the last attempt failed. Red: not live.
    int dot = fresh ? (state == LinkState.OK ? palette.good : palette.warn) : palette.bad;
    if (state == LinkState.JOINING) {
      dot = palette.clay;
    }
    if (dot != shownStatusColor) {
      Ui.fill(statusDot, dot, 4);
      shownStatusColor = dot;
    }

    if (statusShowing) {
      banner.show("", palette.muted);
    } else if (fresh && state == LinkState.OK) {
      // AUTO is toggled from Limits only, so say it is on wherever the cycle has got to.
      String at = TimeFormat.hm(repo.lastGoodWallMs());
      banner.show(String.format(auto ? bannerAuto : bannerUpdated, at), palette.muted);
    } else {
      long since = repo.sinceLastGoodMs();
      String what = getString(state.shortText(repo.linkErr()));
      // While still fresh this is one missed poll: say so quietly. Once stale, say it in clay.
      banner.show(
          since >= 60_000L ? String.format(bannerStale, what, TimeFormat.duration(since)) : what,
          fresh ? palette.muted : palette.clay);
    }

    int ledColor = 0;
    if (fresh && s != null) {
      int worst = s.sessionPct > s.weeklyPct ? s.sessionPct : s.weeklyPct;
      ledColor =
          worst >= palette.badFrom
              ? palette.ledBad
              : (worst >= palette.warnFrom ? palette.ledWarn : 0);
    }
    led.setColor(ledColor);
  }
}
