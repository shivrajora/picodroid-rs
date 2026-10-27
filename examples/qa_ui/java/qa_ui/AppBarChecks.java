// SPDX-License-Identifier: GPL-3.0-only
package qa_ui;

import picodroid.activity.OnBackPressedCallback;
import picodroid.activity.OnBackPressedDispatcher;
import picodroid.app.ActionBar;
import picodroid.lifecycle.Lifecycle;
import picodroid.view.MenuItem;
import picodroid.view.View;
import picodroid.widget.Button;
import picodroid.widget.LinearLayout;
import picodroid.widget.Toolbar;

/**
 * The app bar and Back: {@code Toolbar}, {@code Activity.setActionBar} / {@code getActionBar}, the
 * title from the manifest's {@code android:label}, the up button through {@code
 * onOptionsItemSelected}, and {@code OnBackPressedDispatcher} with its callbacks, plain and
 * lifecycle-bound. In their own class: {@link QaUiActivity} is at the 64 KB class-file limit.
 */
final class AppBarChecks {
  /** A callback that counts its presses. */
  static final class Counting extends OnBackPressedCallback {
    int presses;

    Counting(boolean enabled) {
      super(enabled);
    }

    @Override
    public void handleOnBackPressed() {
      presses++;
    }
  }

  private final QaUiActivity activity;
  private Toolbar bar;

  /** The ids the up button reported to {@code onOptionsItemSelected}. */
  private int homeItems;

  AppBarChecks(QaUiActivity activity) {
    this.activity = activity;
  }

  /** From {@code QaUiActivity.onOptionsItemSelected}: the up button's item. */
  boolean onItem(MenuItem item) {
    if (item.getItemId() == picodroid.R.id.home) {
      homeItems++;
      return true; // taken: the default would finish the QA activity
    }
    return false;
  }

  /** Phase 1's sections. */
  void phase1(LinearLayout root) {
    QaUiActivity.section("appBar", () -> appBar(root));
    QaUiActivity.section("backDispatch", () -> backDispatch());
  }

  /** Phase 2's section, a frame later. */
  void phase2() {
    QaUiActivity.section("appBarLaidOut", () -> appBarLaidOut());
  }

  private void appBar(LinearLayout root) {
    QaUiActivity.check(
        "title from the activity's android:label",
        "UI checks".equals(activity.getTitle().toString()));
    QaUiActivity.check("no action bar until setActionBar", activity.getActionBar() == null);

    bar = new Toolbar(activity);
    bar.setBackgroundColor(0xFF1F8A8A);
    root.addView(bar, new LinearLayout.LayoutParams(LinearLayout.LayoutParams.MATCH_PARENT, 40));
    QaUiActivity.check("an empty bar has no children", bar.getChildCount() == 0);
    activity.setActionBar(bar);
    ActionBar ab = activity.getActionBar();
    QaUiActivity.check("setActionBar installs it", ab != null && ab.isShowing());
    QaUiActivity.check("the bar shows the title", "UI checks".equals(bar.getTitle().toString()));
    QaUiActivity.check(
        "title display is the default", ab.getDisplayOptions() == ActionBar.DISPLAY_SHOW_TITLE);

    activity.setTitle("Renamed");
    QaUiActivity.check(
        "setTitle reaches the bar",
        "Renamed".equals(activity.getTitle().toString())
            && "Renamed".equals(bar.getTitle().toString()));
    ab.setTitle("Bar only");
    QaUiActivity.check(
        "ActionBar.setTitle leaves the Activity's",
        "Bar only".equals(bar.getTitle().toString())
            && "Renamed".equals(activity.getTitle().toString()));
    ab.setSubtitle("second line");
    QaUiActivity.check("subtitle", "second line".equals(ab.getSubtitle().toString()));
    ab.setSubtitle(null);
    ab.hide();
    QaUiActivity.check("hide", !ab.isShowing() && bar.getVisibility() == View.GONE);
    ab.show();
    QaUiActivity.check("show", ab.isShowing());

    int before = bar.getChildCount();
    ab.setDisplayHomeAsUpEnabled(true);
    QaUiActivity.check(
        "home-as-up adds the up button",
        (ab.getDisplayOptions() & ActionBar.DISPLAY_HOME_AS_UP) != 0
            && bar.getChildCount() == before + 1);
    navButton().performClick(); // delivered on a later frame: phase 2 reads it
  }

  /** The up button: the bar's own Button child. */
  private Button navButton() {
    for (int i = 0; i < bar.getChildCount(); i++) {
      if (bar.getChildAt(i) instanceof Button) {
        return (Button) bar.getChildAt(i);
      }
    }
    return null;
  }

  /** After a frame: the up button's tap, and where the bar put its children. */
  void appBarLaidOut() {
    QaUiActivity.check("up reaches onOptionsItemSelected as home", homeItems == 1);
    Button up = navButton();
    QaUiActivity.check(
        "up button is a square at the start",
        up != null && up.getLeft() == 0 && up.getWidth() == 40 && up.getHeight() == 40);
    QaUiActivity.check("bar height", bar.getHeight() == 40);
    activity.getActionBar().setDisplayHomeAsUpEnabled(false);
    activity.setActionBar(null);
    QaUiActivity.check("setActionBar(null) detaches", activity.getActionBar() == null);
  }

  void backDispatch() {
    final int[] fallback = new int[1];
    OnBackPressedDispatcher d = new OnBackPressedDispatcher(() -> fallback[0]++);
    d.onBackPressed();
    QaUiActivity.check(
        "no callback runs the fallback", fallback[0] == 1 && !d.hasEnabledCallbacks());
    Counting a = new Counting(true);
    Counting b = new Counting(true);
    d.addCallback(a);
    d.addCallback(b);
    d.onBackPressed();
    QaUiActivity.check("the newest enabled callback", b.presses == 1 && a.presses == 0);
    b.setEnabled(false);
    d.onBackPressed();
    QaUiActivity.check("a disabled one is passed over", a.presses == 1 && b.presses == 1);
    a.remove();
    d.onBackPressed();
    QaUiActivity.check(
        "removed, then the fallback",
        fallback[0] == 2 && a.presses == 1 && !d.hasEnabledCallbacks());
    b.setEnabled(true);
    QaUiActivity.check("enabled again", d.hasEnabledCallbacks());
    b.remove();
    new OnBackPressedDispatcher().onBackPressed(); // no fallback: nothing happens

    // Bound to a lifecycle: heard between ON_START and ON_STOP, gone at ON_DESTROY.
    Lifecycle owner = new Lifecycle();
    Counting c = new Counting(true);
    d.addCallback(owner, c);
    d.onBackPressed();
    QaUiActivity.check("not before ON_START", c.presses == 0 && fallback[0] == 3);
    owner.setCurrentState(Lifecycle.State.RESUMED);
    d.onBackPressed();
    QaUiActivity.check("heard once started", c.presses == 1);
    owner.setCurrentState(Lifecycle.State.CREATED);
    d.onBackPressed();
    QaUiActivity.check("not once stopped", c.presses == 1 && fallback[0] == 4);
    owner.setCurrentState(Lifecycle.State.STARTED);
    d.onBackPressed();
    QaUiActivity.check("heard again on restart", c.presses == 2);
    owner.setCurrentState(Lifecycle.State.DESTROYED);
    d.onBackPressed();
    QaUiActivity.check("dropped on ON_DESTROY", c.presses == 2 && fallback[0] == 5);
    Counting late = new Counting(true);
    d.addCallback(owner, late);
    d.onBackPressed();
    QaUiActivity.check("a destroyed owner adds nothing", late.presses == 0 && fallback[0] == 6);

    // The Activity's own: one dispatcher, and BACK goes through it.
    OnBackPressedDispatcher mine = activity.getOnBackPressedDispatcher();
    QaUiActivity.check(
        "one dispatcher per Activity", mine == activity.getOnBackPressedDispatcher());
    Counting keep = new Counting(true);
    mine.addCallback(keep);
    activity.onBackPressed();
    QaUiActivity.check("onBackPressed reaches the callback", keep.presses == 1);
    // Left in place: a stray BACK during the run must not finish the QA activity.
  }
}
