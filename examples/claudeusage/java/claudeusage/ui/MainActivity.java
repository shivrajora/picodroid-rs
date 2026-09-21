// SPDX-License-Identifier: GPL-3.0-only
package claudeusage.ui;

import claudeusage.ClaudeUsageApp;
import claudeusage.data.LinkState;
import claudeusage.data.UsageRepository;
import claudeusage.data.UsageSnapshot;
import claudeusage.hardware.RgbLed;
import claudeusage.util.TimeFormat;
import picodroid.app.Activity;
import picodroid.concurrent.Executors;
import picodroid.graphics.drawable.GradientDrawable;
import picodroid.os.Bundle;
import picodroid.util.Log;
import picodroid.view.KeyEvent;
import picodroid.view.OnKeyListener;
import picodroid.view.View;
import picodroid.widget.Button;
import picodroid.widget.FrameLayout;

/**
 * The one Activity. Four screens live in it as pages rather than as Activities of their own: one
 * key listener, one header and footer, and a page switch is a fade instead of a lifecycle.
 *
 * <p>Buttons, with the display landscape (A top-left, B bottom-left, X top-right, Y bottom-right);
 * each corner of the screen carries the hint for the button beside it:
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
public class MainActivity extends Activity implements OnKeyListener, UsageRepository.Listener {
  private static final String TAG = ClaudeUsageApp.TAG;

  private static final int PAGE_LIMITS = 0;
  private static final int PAGE_COUNT = 4;
  private static final int AUTO_SECONDS = 10;
  private static final int FADE_MS = 180;

  /** LED colours, deliberately dim. Off while usage is comfortable or the data is stale. */
  private static final int LED_WARN = 0x281400;

  private static final int LED_BAD = 0x300000;

  private final UsageRepository repo = UsageRepository.get();
  private RgbLed led;

  private FrameLayout root;
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

  private Page page;
  private boolean pageBuilt;
  private boolean pageIsStatus;
  private int pageIndex = PAGE_LIMITS;
  private int buildToken;
  private boolean auto;
  private int autoSeconds;
  private boolean destroyed;

  @Override
  protected void onCreate(Bundle savedInstanceState) {
    super.onCreate(savedInstanceState);
    getDisplay();
    led = RgbLed.open();

    root = Ui.group(0, 0, Ui.WIDTH, Ui.HEIGHT, Palette.BACKGROUND);
    buildHeader();
    buildFooter();

    // Keys reach Java only through a focused view, so this invisible one holds the focus.
    keyCatcher = new Button("");
    keyCatcher.setSize(1, 1);
    keyCatcher.setPosition(0, 0);
    keyCatcher.setAlpha(0f);
    keyCatcher.setBackground(
        new GradientDrawable().setColor(Palette.BACKGROUND).setCornerRadius(0));
    keyCatcher.setOnKeyListener(this);
    root.addView(keyCatcher);

    setContentView(root);
    keyCatcher.requestFocus();

    showPage();
    Log.i(TAG, "ui ready");
  }

  private void buildHeader() {
    Ui.label(root, "prev", Ui.MARGIN, 4, Palette.FAINT);
    title = new Line(Ui.label(root, "", 50, 4, Palette.TEXT), "", Palette.TEXT);
    plan = new Line(Ui.label(root, "", 150, 4, Palette.CLAY), "", Palette.CLAY);
    clock = new Line(Ui.labelRight(root, "", 196, 4, 48, Palette.MUTED), "", Palette.MUTED);
    statusDot = Ui.box(252, 9, 8, 8, Palette.FAINT, 4);
    shownStatusColor = Palette.FAINT;
    root.addView(statusDot);
    syncHint =
        new Line(Ui.labelRight(root, "sync", 268, 4, 44, Palette.FAINT), "sync", Palette.FAINT);
  }

  private void buildFooter() {
    int y = Ui.HEIGHT - Ui.FOOTER_HEIGHT + 3;
    Ui.label(root, "next", Ui.MARGIN, y, Palette.FAINT);
    for (int i = 0; i < PAGE_COUNT; i++) {
      pageDots[i] = Ui.box(50 + i * 11, y + 6, 6, 6, Palette.TRACK, 3);
      root.addView(pageDots[i]);
    }
    banner = new Line(Ui.labelCentred(root, "", 98, y, 166, Palette.MUTED), "", Palette.MUTED);
    homeHint =
        new Line(Ui.labelRight(root, "auto", 268, y, 44, Palette.FAINT), "auto", Palette.FAINT);
  }

  // ── Lifecycle ──────────────────────────────────────────────────────────────

  @Override
  public void onResume() {
    super.onResume();
    repo.setListener(this);
    refresh();
  }

  @Override
  public void onPause() {
    repo.setListener(null);
    super.onPause();
  }

  @Override
  public void onDestroy() {
    destroyed = true;
    buildToken++;
    repo.setListener(null);
    led.setColor(0);
    super.onDestroy();
  }

  @Override
  public void onBackPressed() {
    // Y is handled in onKey; never finish().
  }

  // ── Input ──────────────────────────────────────────────────────────────────

  @Override
  public boolean onKey(View v, KeyEvent event) {
    if (event.getAction() != KeyEvent.ACTION_DOWN) {
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

  private void turnPage(int by) {
    pageIndex = (pageIndex + by + PAGE_COUNT) % PAGE_COUNT;
    autoSeconds = 0;
    showPage();
  }

  // ── Pages ──────────────────────────────────────────────────────────────────

  private boolean hasData() {
    UsageSnapshot s = repo.snapshot();
    return s != null && s.hasLimits();
  }

  private void showPage() {
    final int token = ++buildToken;
    discardPage();
    pageIsStatus = !hasData();
    page = pageIsStatus ? new StatusPage() : create(pageIndex);
    pageBuilt = false;
    page.root.setAlpha(0f);
    root.addView(page.root);
    Log.i(TAG, "page -> " + page.title());
    refreshChrome();
    Executors.mainExecutor().execute(() -> continueBuild(token));
  }

  /**
   * removeView, not close(): close() frees the widget but leaves the view in the parent's child
   * list, and the whole page tree then stays reachable for as long as the Activity lives.
   */
  private void discardPage() {
    if (page != null) {
      root.removeView(page.root);
      page = null;
    }
    pageBuilt = false;
  }

  private static Page create(int index) {
    switch (index) {
      case 1:
        return new ModelsPage();
      case 2:
        return new BurnPage();
      case 3:
        return new HistoryPage();
      default:
        return new LimitsPage();
    }
  }

  private void continueBuild(int token) {
    if (token != buildToken || destroyed || page == null) {
      return; // a newer page took over, or the screen is gone
    }
    try {
      if (page.buildNext()) {
        Executors.mainExecutor().execute(() -> continueBuild(token));
        return;
      }
      pageBuilt = true;
      page.update(repo, System.currentTimeMillis());
      page.root.animate().alpha(1f).setDuration(FADE_MS).start();
    } catch (OutOfMemoryError | RuntimeException e) {
      // A half-built page would stay invisible for good. Drop it; the next tick builds it again,
      // by which time the collector has had a chance to run.
      Log.w(TAG, "page build failed: " + e);
      discardPage();
    }
  }

  // ── Repository callbacks (main thread) ─────────────────────────────────────

  @Override
  public void onUsageChanged() {
    refresh();
  }

  @Override
  public void onTick() {
    if (auto && !pageIsStatus && repo.isFresh() && ++autoSeconds >= AUTO_SECONDS) {
      turnPage(1);
      return;
    }
    refresh();
  }

  private void refresh() {
    if (destroyed) {
      return;
    }
    if (page == null || pageIsStatus == hasData()) {
      showPage(); // first data arrived, or the last build failed
      return;
    }
    refreshChrome();
    if (pageBuilt) {
      page.update(repo, System.currentTimeMillis());
    }
  }

  /** Header, footer and LED: everything outside the page. */
  private void refreshChrome() {
    UsageSnapshot s = repo.snapshot();
    boolean fresh = repo.isFresh();
    int state = repo.linkState();
    long now = System.currentTimeMillis();

    title.show(page != null ? page.title() : "", Palette.TEXT);
    plan.show(s != null && !pageIsStatus ? s.plan.toUpperCase() : "", Palette.CLAY);
    clock.show(s != null ? TimeFormat.hm(now) : "", Palette.MUTED);
    syncHint.show("sync", repo.isSyncing() ? Palette.CLAY : Palette.FAINT);
    homeHint.show(
        pageIndex == PAGE_LIMITS || pageIsStatus ? "auto" : "home",
        auto && pageIndex == PAGE_LIMITS ? Palette.CLAY : Palette.FAINT);

    // Green: live. Amber: live numbers, but the last attempt failed. Red: not live.
    int dot = fresh ? (state == LinkState.OK ? Palette.GOOD : Palette.WARN) : Palette.BAD;
    if (state == LinkState.JOINING) {
      dot = Palette.CLAY;
    }
    if (dot != shownStatusColor) {
      Ui.fill(statusDot, dot, 4);
      shownStatusColor = dot;
    }

    int activeDot = pageIsStatus ? -1 : pageIndex;
    if (activeDot != shownPageDot) {
      for (int i = 0; i < PAGE_COUNT; i++) {
        Ui.fill(pageDots[i], i == activeDot ? Palette.CLAY : Palette.TRACK, 3);
      }
      shownPageDot = activeDot;
    }

    if (pageIsStatus) {
      banner.show("", Palette.MUTED);
    } else if (fresh && state == LinkState.OK) {
      // AUTO is toggled from Limits only, so say it is on wherever the cycle has got to.
      banner.show(
          (auto ? "auto  -  " : "updated ") + TimeFormat.hm(repo.lastGoodWallMs()), Palette.MUTED);
    } else {
      long since = repo.sinceLastGoodMs();
      String what = LinkState.shortText(state, repo.linkErr());
      // While still fresh this is one missed poll: say so quietly. Once stale, say it in clay.
      banner.show(
          since >= 60_000L ? what + " - " + TimeFormat.duration(since) : what,
          fresh ? Palette.MUTED : Palette.CLAY);
    }

    int ledColor = 0;
    if (fresh && s != null) {
      int worst = s.sessionPct > s.weeklyPct ? s.sessionPct : s.weeklyPct;
      ledColor = worst >= Palette.BAD_FROM ? LED_BAD : (worst >= Palette.WARN_FROM ? LED_WARN : 0);
    }
    led.setColor(ledColor);
  }
}
