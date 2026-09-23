// SPDX-License-Identifier: GPL-3.0-only
package claudeusage.ui;

import claudeusage.R;
import claudeusage.data.LinkState;
import claudeusage.data.UsageService;
import claudeusage.data.UsageSnapshot;
import claudeusage.hardware.RgbLed;
import claudeusage.util.TimeFormat;
import picodroid.app.Activity;
import picodroid.concurrent.Executors;
import picodroid.content.Intent;
import picodroid.content.ServiceConnection;
import picodroid.content.res.Resources;
import picodroid.os.Bundle;
import picodroid.os.IBinder;
import picodroid.util.Log;
import picodroid.view.KeyEvent;
import picodroid.view.OnKeyListener;
import picodroid.view.View;
import picodroid.widget.Button;
import picodroid.widget.FrameLayout;
import picodroid.widget.LinearLayout;

/**
 * The one Activity, declared in the manifest as the entry point. Four screens live in it as pages
 * rather than as Activities of their own: one key listener, one header and footer, and a page
 * switch is a fade instead of a lifecycle. The chrome is {@code res/layout/activity_main.xml}.
 *
 * <p>The numbers come from {@link UsageService}, started here so they stay warm and bound while the
 * screen is on. Buttons, with the display landscape (A top-left, B bottom-left, X top-right, Y
 * bottom-right); each corner of the screen carries the hint for the button beside it:
 *
 * <ul>
 *   <li>A previous screen, B next screen (both wrap)
 *   <li>X sync now
 *   <li>Y home (Limits); on Limits, toggle AUTO, which cycles the screens
 * </ul>
 *
 * Y never leaves the app: this is an appliance, and BACK falling through to finish() would drop it
 * to a launcher nobody asked for.
 */
public class MainActivity extends Activity implements OnKeyListener, UsageService.Listener {
  private static final String TAG = UsageService.TAG;

  private static final int PAGE_LIMITS = 0;
  private static final int PAGE_COUNT = 4;

  private static final String STATE_PAGE = "page";
  private static final String STATE_AUTO = "auto";

  /** AUTO is a setting: it survives a power cycle. */
  private static final String KEY_AUTO = "auto";

  private static final int[] PAGE_DOT_IDS = {
    R.id.page_dot_0, R.id.page_dot_1, R.id.page_dot_2, R.id.page_dot_3
  };

  private final ServiceConnection connection =
      new ServiceConnection() {
        @Override
        public void onServiceConnected(IBinder binder) {
          repo = ((UsageService.LocalBinder) binder).service;
          repo.setListener(MainActivity.this);
          refresh();
        }

        @Override
        public void onServiceDisconnected() {
          repo = null;
        }
      };

  /** Null until the Service is bound; every callback that reads it arrives after that. */
  private UsageService repo;

  private Palette palette;
  private int autoPeriod;
  private int fadeMs;
  private RgbLed led;

  private FrameLayout pageHost;
  private Button keyCatcher;
  private Line title;
  private Line plan;
  private Line clock;
  private Line syncHint;
  private Line homeHint;
  private Line banner;
  private FrameLayout statusDot;
  private final FrameLayout[] pageDots = new FrameLayout[PAGE_COUNT];
  private int shownStatusColor;
  private int shownPageDot = -1;
  private String hintAuto;
  private String hintHome;
  private String bannerAuto;
  private String bannerUpdated;
  private String bannerStale;

  private Page page;
  private boolean pageBuilt;
  private boolean pageIsStatus;
  private int pageIndex = PAGE_LIMITS;
  private int buildToken;
  private int builtToken;
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
    bannerAuto = getString(R.string.banner_auto);
    bannerUpdated = getString(R.string.banner_updated);
    bannerStale = getString(R.string.banner_stale);
    led = RgbLed.open();

    setContentView(R.layout.activity_main);
    bindChrome();

    if (savedInstanceState != null) {
      pageIndex = savedInstanceState.getInt(STATE_PAGE, PAGE_LIMITS);
      auto = savedInstanceState.getBoolean(STATE_AUTO, false);
    } else {
      auto = getSharedPreferences(UsageService.PREFS, MODE_PRIVATE).getBoolean(KEY_AUTO, false);
    }

    startService(new Intent(UsageService.class));
    Log.i(TAG, "ui ready");
  }

  /** Finds the chrome and strips the theme border from its containers. */
  private void bindChrome() {
    int bg = palette.background;
    Ui.flat(findViewById(R.id.root), bg);
    Ui.flat(findViewById(R.id.header), bg);
    Ui.flat(findViewById(R.id.clock_box), bg);
    Ui.flat(findViewById(R.id.dot_box), bg);
    Ui.flat(findViewById(R.id.sync_box), bg);
    Ui.flat(findViewById(R.id.footer), bg);
    Ui.flat(findViewById(R.id.page_dots), bg);
    Ui.flat(findViewById(R.id.banner_box), bg);
    Ui.flat(findViewById(R.id.home_box), bg);
    pageHost = findViewById(R.id.page_host);
    Ui.flat(pageHost, bg);

    title = new Line(findViewById(R.id.title), palette.text);
    plan = new Line(findViewById(R.id.plan), palette.clay);
    clock = new Line(findViewById(R.id.clock), palette.muted);
    syncHint = new Line(findViewById(R.id.hint_sync), getString(R.string.hint_sync), palette.faint);
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

    keyCatcher = findViewById(R.id.key_catcher);
    keyCatcher.setOnKeyListener(this);
    keyCatcher.requestFocus();
  }

  // ── Lifecycle ──────────────────────────────────────────────────────────────

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
    buildToken++;
    led.close();
    super.onDestroy();
  }

  @Override
  protected void onSaveInstanceState(Bundle outState) {
    super.onSaveInstanceState(outState);
    outState.putInt(STATE_PAGE, pageIndex);
    outState.putBoolean(STATE_AUTO, auto);
  }

  @Override
  public void onBackPressed() {
    // Y is handled in onKey; never finish().
  }

  // ── Input ──────────────────────────────────────────────────────────────────

  @Override
  public boolean onKey(View v, KeyEvent event) {
    if (event.getAction() != KeyEvent.ACTION_DOWN || repo == null) {
      return true;
    }
    int code = event.getKeyCode();
    if (pageIsStatus && code != KeyEvent.KEYCODE_DPAD_CENTER) {
      return true; // nothing to page through yet; X still retries
    }
    switch (code) {
      case KeyEvent.KEYCODE_DPAD_UP:
        turnPage(-1);
        break;
      case KeyEvent.KEYCODE_DPAD_DOWN:
        turnPage(1);
        break;
      case KeyEvent.KEYCODE_DPAD_CENTER:
        Log.i(TAG, "sync requested");
        repo.refreshNow();
        break;
      case KeyEvent.KEYCODE_BACK:
        if (pageIndex == PAGE_LIMITS) {
          auto = !auto;
          autoSeconds = 0;
          Log.i(TAG, auto ? "auto on" : "auto off");
          saveAuto(auto);
          refreshChrome();
        } else {
          pageIndex = PAGE_LIMITS;
          showPage();
        }
        break;
      default:
        break;
    }
    return true;
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

  private void turnPage(int by) {
    pageIndex = (pageIndex + by + PAGE_COUNT) % PAGE_COUNT;
    autoSeconds = 0;
    showPage();
  }

  // ── Pages ──────────────────────────────────────────────────────────────────

  private boolean hasData() {
    UsageSnapshot s = repo == null ? null : repo.snapshot();
    return s != null && s.hasLimits();
  }

  /**
   * Replaces the page over several UI ticks: tearing down the old tree, repainting the chrome and
   * starting the new page each cost the RP2350 tens of milliseconds, and together in one tick they
   * overran the slow-handler budget on every page turn.
   */
  private void showPage() {
    final int token = ++buildToken;
    pageBuilt = false;
    Executors.mainExecutor().execute(() -> replacePage(token));
  }

  private void replacePage(int token) {
    if (token != buildToken || destroyed) {
      return;
    }
    discardPage();
    Executors.mainExecutor().execute(() -> startPage(token));
  }

  private void startPage(int token) {
    if (token != buildToken || destroyed || repo == null) {
      return;
    }
    pageIsStatus = !hasData();
    builtToken = token;
    page = pageIsStatus ? new StatusPage(this, palette, repo.bridgeAddress()) : create(pageIndex);
    page.root.setAlpha(0f);
    pageHost.addView(page.root);
    Log.i(TAG, "page -> " + getString(page.titleRes()));
    refreshPageChrome();
    Executors.mainExecutor().execute(() -> continueBuild(token));
  }

  /**
   * removeView, not close(): close() frees the widget but leaves the view in the parent's child
   * list, and the whole page tree then stays reachable for as long as the Activity lives.
   */
  private void discardPage() {
    if (page != null) {
      pageHost.removeView(page.root);
      page = null;
    }
    pageBuilt = false;
  }

  /** Between showPage() and the new page existing; a failed build leaves no swap in flight. */
  private boolean swapInFlight() {
    return buildToken != builtToken;
  }

  private Page create(int index) {
    switch (index) {
      case 1:
        return new ModelsPage(this, palette);
      case 2:
        return new BurnPage(this, palette);
      case 3:
        return new HistoryPage(this, palette);
      default:
        return new LimitsPage(this, palette);
    }
  }

  private void continueBuild(int token) {
    if (token != buildToken || destroyed || page == null || repo == null) {
      return; // a newer page took over, or the screen is gone
    }
    try {
      if (page.buildNext()) {
        Executors.mainExecutor().execute(() -> continueBuild(token));
        return;
      }
      pageBuilt = true;
      page.update(repo, System.currentTimeMillis());
      page.root.animate().alpha(1f).setDuration(fadeMs).start();
    } catch (OutOfMemoryError | RuntimeException e) {
      // A half-built page would stay invisible for good. Drop it; the next tick builds it again,
      // by which time the collector has had a chance to run.
      Log.w(TAG, "page build failed: " + e);
      discardPage();
    }
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
    if (auto && !pageIsStatus && repo.isFresh() && ++autoSeconds >= autoPeriod) {
      turnPage(1);
      return;
    }
    // A full repaint builds a dozen strings just to find that none changed, which costs the
    // RP2350 some 55 ms: over the slow-handler budget, every second. Everything on the data
    // screens is minute-grained, so repaint only when the minute, the link or the freshness moved.
    long minute = System.currentTimeMillis() / 60_000L;
    LinkState state = repo.linkState();
    boolean fresh = repo.isFresh();
    if (pageIsStatus || minute != tickMinute || state != tickState || fresh != tickFresh) {
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
    if ((page == null && !swapInFlight()) || (page != null && pageIsStatus == hasData())) {
      refreshChrome(); // the page turn itself only repaints the title and dots
      showPage(); // first data arrived, or the last build failed
      return;
    }
    refreshChrome();
    if (pageBuilt) {
      page.update(repo, System.currentTimeMillis());
    }
  }

  /** The part of the chrome a page turn changes; cheap enough to share a tick with the turn. */
  private void refreshPageChrome() {
    title.show(page != null ? getString(page.titleRes()) : "", palette.text);
    homeHint.show(
        pageIndex == PAGE_LIMITS || pageIsStatus ? hintAuto : hintHome,
        auto && pageIndex == PAGE_LIMITS ? palette.clay : palette.faint);
    int activeDot = pageIsStatus ? -1 : pageIndex;
    if (activeDot != shownPageDot) {
      for (int i = 0; i < PAGE_COUNT; i++) {
        Ui.fill(pageDots[i], i == activeDot ? palette.clay : palette.track, 3);
      }
      shownPageDot = activeDot;
    }
  }

  /** Header, footer and LED: everything outside the page. */
  private void refreshChrome() {
    UsageSnapshot s = repo.snapshot();
    boolean fresh = repo.isFresh();
    LinkState state = repo.linkState();
    long now = System.currentTimeMillis();

    refreshPageChrome();
    plan.show(s != null && !pageIsStatus ? s.plan.toUpperCase() : "", palette.clay);
    clock.show(s != null ? TimeFormat.hm(now) : "", palette.muted);
    syncHint.show(getString(R.string.hint_sync), repo.isSyncing() ? palette.clay : palette.faint);

    // Green: live. Amber: live numbers, but the last attempt failed. Red: not live.
    int dot = fresh ? (state == LinkState.OK ? palette.good : palette.warn) : palette.bad;
    if (state == LinkState.JOINING) {
      dot = palette.clay;
    }
    if (dot != shownStatusColor) {
      Ui.fill(statusDot, dot, 4);
      shownStatusColor = dot;
    }

    if (pageIsStatus) {
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
