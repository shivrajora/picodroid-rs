// SPDX-License-Identifier: GPL-3.0-only
package fragmentdemo;

import picodroid.app.Activity;
import picodroid.app.Fragment;
import picodroid.app.FragmentFactory;
import picodroid.app.FragmentManager;
import picodroid.app.FragmentTransaction;
import picodroid.content.Intent;
import picodroid.os.Bundle;
import picodroid.os.SystemClock;
import picodroid.util.Log;
import picodroid.view.View;
import picodroid.view.ViewGroup;

/**
 * Conformance driver for {@code picodroid.app.Fragment}: the callback order against the host's,
 * replace with a back stack, hide/show, detach/attach, a headless fragment, the refused
 * transactions, ten replace-and-pop rounds, a BACK that pops instead of finishing, then a covered
 * and reclaimed host whose fragments, arguments, saved state and back stack come back through the
 * {@link FragmentFactory}, and a second BACK on the restored stack. Instances trace as {@code
 * H<n>}; the sim row's test.ctrl sends the two BACKs when the log says {@code ready for back}.
 */
public class MainActivity extends Activity {
  private static final String TAG = T.TAG;
  private static final int BAD_CONTAINER = 0x7f0fffff;

  private static int instances;
  private static long switchStartMs;

  /** Which BACK the driver is waiting for (1 or 2); static so it survives the re-creation. */
  private static int stage;

  private int id;
  private boolean resumedOnce;
  private FragmentManager fm;
  private HomeFragment home;
  private DetailFragment detail;
  private int backStackChanges;

  private final FragmentManager.OnBackStackChangedListener onBackStack =
      () -> {
        backStackChanges++;
        if (fm.getBackStackEntryCount() == 0) {
          if (stage == 1) {
            stage = 0;
            T.later(30, () -> afterBack1());
          } else if (stage == 2) {
            stage = 0;
            T.later(30, () -> afterBack2());
          }
        }
      };

  @Override
  protected void onCreate(Bundle savedInstanceState) {
    // Before super.onCreate, as on Android: the restore runs inside it.
    getSupportFragmentManager()
        .setFragmentFactory(
            new FragmentFactory() {
              @Override
              public Fragment instantiate(String className) {
                T.log("factory");
                if (className.equals(HomeFragment.class.getName())) {
                  return new HomeFragment();
                }
                if (className.equals(DetailFragment.class.getName())) {
                  return new DetailFragment();
                }
                return super.instantiate(className);
              }
            });
    super.onCreate(savedInstanceState);
    id = ++instances;
    fm = getSupportFragmentManager();
    fm.addOnBackStackChangedListener(onBackStack);
    setContentView(R.layout.activity_main);
    T.log("H" + id + (savedInstanceState == null ? ".onCreate(null)" : ".onCreate(saved)"));
    if (savedInstanceState == null) {
      home = new HomeFragment();
      fm.beginTransaction().add(R.id.container, home, "home").commit();
      T.check("commit is deferred", fm.findFragmentByTag("home") == null);
      T.check("executePendingTransactions ran it", fm.executePendingTransactions());
      T.check(
          "home created, no view yet",
          home.isAdded() && home.getView() == null && !home.isResumed());
      T.check("found by tag", fm.findFragmentByTag("home") == home);
      T.check("found by id", fm.findFragmentById(R.id.container) == home);
    } else {
      home = (HomeFragment) fm.findFragmentByTag("home");
      detail = (DetailFragment) fm.findFragmentByTag("detail");
      T.check("back stack restored", fm.getBackStackEntryCount() == 1);
      T.check("factory made both", T.count("factory") == 2);
      T.check(
          "home restored at CREATED with its state",
          home != null && home.n == 2 && home.counter == 42 && !home.isAdded());
      T.check(
          "detail restored, added, with its arguments",
          detail != null
              && detail.n == 13
              && detail.isAdded()
              && detail.requireArguments().getInt("n") == 7);
    }
  }

  @Override
  public void onStart() {
    super.onStart();
    T.log("H" + id + ".onStart");
  }

  @Override
  public void onResume() {
    super.onResume();
    T.log("H" + id + ".onResume");
    if (resumedOnce) {
      return;
    }
    resumedOnce = true;
    if (id == 1) {
      T.later(50, () -> runSteps());
    } else {
      T.later(50, () -> afterRecreate());
    }
  }

  @Override
  public void onPause() {
    T.log("H" + id + ".onPause");
    super.onPause();
  }

  @Override
  public void onStop() {
    T.log("H" + id + ".onStop");
    super.onStop();
  }

  @Override
  protected void onSaveInstanceState(Bundle outState) {
    super.onSaveInstanceState(outState);
    T.log("H" + id + ".onSaveInstanceState");
  }

  @Override
  protected void onRestoreInstanceState(Bundle savedInstanceState) {
    super.onRestoreInstanceState(savedInstanceState);
    T.log("H" + id + ".onRestoreInstanceState");
  }

  @Override
  public void onDestroy() {
    T.log("H" + id + ".onDestroy");
    super.onDestroy();
  }

  // ── The script ─────────────────────────────────────────────────────────

  private void runSteps() {
    try {
      stepOrder();
      stepReplace();
      stepHideShow();
      stepDetachAttach();
      stepHeadless();
      stepRefusals();
      stepChurn();
      stage = 1;
      Log.i(TAG, "ready for back 1");
    } catch (RuntimeException e) {
      T.check("steps threw " + e, false);
      T.report();
      finish();
    }
  }

  private void stepOrder() {
    T.check(
        "callback order up",
        T.before("Home1.onAttach", 1, "Home1.onCreate(null)", 1)
            && T.before("Home1.onCreate(null)", 1, "Home1.onCreateView(null)", 1)
            && T.before("Home1.onCreateView(null)", 1, "Home1.onViewCreated", 1)
            && T.before("Home1.onViewCreated", 1, "Home1.onStart", 1)
            && T.before("Home1.onStart", 1, "Home1.onResume", 1));
    T.check(
        "fragment created before host onStart",
        T.before("Home1.onCreate(null)", 1, "H1.onStart", 1));
    T.check(
        "fragment started before host onStart body", T.before("Home1.onStart", 1, "H1.onStart", 1));
    T.check(
        "fragment resumed after host onResume body",
        T.before("H1.onResume", 1, "Home1.onResume", 1));
    T.check("home resumed and visible", home.isResumed() && home.isVisible());
    ViewGroup container = findViewById(R.id.container);
    T.check(
        "home view in the container",
        container.getChildCount() == 1 && container.getChildAt(0) == home.getView());
  }

  private void stepReplace() {
    ViewGroup container = findViewById(R.id.container);
    detail = new DetailFragment();
    long t0 = SystemClock.elapsedRealtime();
    fm.beginTransaction().replace(R.id.container, detail, "detail").addToBackStack("d1").commit();
    fm.executePendingTransactions();
    Log.i(TAG, "replace took " + (SystemClock.elapsedRealtime() - t0) + " ms");
    T.check(
        "home view destroyed, instance kept",
        T.count("Home1.onDestroyView") == 1
            && T.count("Home1.onDestroy") == 0
            && home.getView() == null
            && !home.isAdded()
            && fm.findFragmentByTag("home") == home);
    T.check(
        "outgoing down before incoming up",
        T.before("Home1.onDestroyView", 1, "Detail1.onAttach", 1));
    T.check("detail resumed and visible", detail.isResumed() && detail.isVisible());
    T.check("back stack has one entry", fm.getBackStackEntryCount() == 1);
    T.check(
        "container swapped",
        container.getChildCount() == 1 && container.getChildAt(0) == detail.getView());
  }

  private void stepHideShow() {
    fm.beginTransaction().hide(detail).commitNow();
    T.check(
        "hidden: GONE, still resumed",
        detail.isHidden()
            && !detail.isVisible()
            && detail.isResumed()
            && detail.getView().getVisibility() == View.GONE
            && T.count("Detail1.onHiddenChanged(true)") == 1);
    fm.beginTransaction().show(detail).commitNow();
    T.check(
        "shown again",
        !detail.isHidden() && detail.isVisible() && T.count("Detail1.onHiddenChanged(false)") == 1);
  }

  private void stepDetachAttach() {
    ViewGroup container = findViewById(R.id.container);
    View old = detail.getView();
    fm.beginTransaction().detach(detail).commitNow();
    T.check(
        "detached: view freed, instance kept",
        detail.isDetached()
            && detail.getView() == null
            && container.getChildCount() == 0
            && T.count("Detail1.onDestroyView") == 1
            && T.count("Detail1.onDestroy") == 0
            && fm.findFragmentByTag("detail") == detail);
    fm.beginTransaction().attach(detail).commitNow();
    T.check(
        "attached with a new view",
        !detail.isDetached()
            && detail.isResumed()
            && detail.getView() != null
            && detail.getView() != old
            && T.count("Detail1.onCreateView(null)") == 2);
  }

  private void stepHeadless() {
    HeadlessFragment hl = new HeadlessFragment();
    fm.beginTransaction().add(hl, "hl").commitNow();
    T.check(
        "headless resumed without a view",
        hl.isAdded() && hl.isResumed() && hl.getView() == null && !hl.isVisible());
    fm.beginTransaction().hide(hl).commitNow();
    T.check("headless hidden", hl.isHidden() && T.count("Headless1.onHiddenChanged(true)") == 1);
    fm.beginTransaction().remove(hl).commitNow();
    T.check(
        "headless destroyed",
        T.count("Headless1.onDestroy") == 1
            && T.count("Headless1.onDetach") == 1
            && fm.findFragmentByTag("hl") == null
            && !hl.isAdded());
  }

  private void stepRefusals() {
    T.check(
        "adding an added fragment refused",
        throwsState(() -> fm.beginTransaction().add(R.id.container, detail).commitNow()));
    HeadlessFragment lost = new HeadlessFragment();
    T.check(
        "unknown container refused",
        throwsArgument(() -> fm.beginTransaction().add(BAD_CONTAINER, lost, "lost").commitNow()));
    fm.beginTransaction().remove(lost).commitNow();
    T.check(
        "manager usable after the failure",
        T.count("Headless2.onDestroy") == 1
            && fm.findFragmentByTag("lost") == null
            && detail.isResumed());
    T.check(
        "replace into id 0 refused",
        throwsArgument(() -> fm.beginTransaction().replace(0, new HeadlessFragment())));
    FragmentTransaction t = fm.beginTransaction().hide(detail);
    t.commit();
    T.check("second commit refused", throwsState(() -> t.commit()));
    fm.executePendingTransactions();
    T.check("queued hide ran", detail.isHidden());
    fm.beginTransaction().show(detail).commitNow();
  }

  private void stepChurn() {
    backStackChanges = 0;
    boolean popped = true;
    for (int i = 0; i < 10; i++) {
      fm.beginTransaction()
          .replace(R.id.container, new DetailFragment(), "detail")
          .addToBackStack(null)
          .commit();
      fm.executePendingTransactions();
      popped = fm.popBackStackImmediate() && popped;
    }
    T.check("every churn round popped", popped);
    T.check(
        "churn left the first entry and the first detail",
        fm.getBackStackEntryCount() == 1
            && fm.findFragmentByTag("detail") == detail
            && detail.isResumed());
    T.check("churn told the listener", backStackChanges == 20);
    T.check(
        "churn destroyed the transients only",
        T.count("Detail11.onDetach") == 1 && T.count("Home1.onCreate(null)") == 1);
  }

  private void afterBack1() {
    T.check(
        "BACK popped detail",
        T.count("Detail1.onDestroy") == 1
            && T.count("Detail1.onDetach") == 1
            && fm.findFragmentByTag("detail") == null);
    T.check(
        "home rebuilt",
        home.isResumed() && home.getView() != null && T.count("Home1.onCreateView(null)") == 2);
    T.check("back stack empty", fm.getBackStackEntryCount() == 0);
    T.check("BACK did not finish the Activity", T.count("H1.onDestroy") == 0);
    Log.i(TAG, "popped 1");
    stepCover();
  }

  private void stepCover() {
    DetailFragment d = new DetailFragment();
    Bundle args = new Bundle();
    args.putInt("n", 7);
    d.setArguments(args);
    fm.beginTransaction().replace(R.id.container, d, "detail").addToBackStack("d2").commit();
    fm.executePendingTransactions();
    home.counter = 42;
    T.check("arguments kept", d.getArguments() == args && d.n == 12);
    switchStartMs = SystemClock.elapsedRealtime();
    Log.i(TAG, "activity switch start");
    startActivity(new Intent(SecondActivity.class));
  }

  private void afterRecreate() {
    Log.i(
        TAG, "activity round trip took " + (SystemClock.elapsedRealtime() - switchStartMs) + " ms");
    T.check(
        "first host saved and reclaimed",
        T.count("H1.onSaveInstanceState") == 1 && T.count("H1.onDestroy") == 1);
    T.check(
        "fragments saved with the host",
        T.count("Home1.onSaveInstanceState") == 1 && T.count("Detail12.onSaveInstanceState") == 1);
    T.check(
        "fragments torn down before the host's onDestroy",
        T.count("Home1.onDetach") == 1
            && T.count("Detail12.onDetach") == 1
            && T.before("Detail12.onDestroyView", 1, "H1.onDestroy", 1));
    T.check(
        "detail rebuilt and resumed",
        detail.isResumed() && T.count("Detail13.onCreateView(null)") == 1);
    T.check(
        "home waits in the back stack",
        !home.isAdded() && home.getView() == null && T.count("Home2.onCreate(saved)") == 1);
    Log.i(TAG, "recreated with 2 fragments");
    stage = 2;
    Log.i(TAG, "ready for back 2");
  }

  private void afterBack2() {
    T.check(
        "BACK popped the restored detail",
        T.count("Detail13.onDetach") == 1 && fm.findFragmentByTag("detail") == null);
    T.check(
        "home rebuilt with its saved state",
        home.isResumed() && T.count("Home2.onCreateView(saved)") == 1 && home.counter == 42);
    T.check("back stack empty again", fm.getBackStackEntryCount() == 0);
    T.report();
    finish();
  }

  private static boolean throwsState(Runnable r) {
    try {
      r.run();
      return false;
    } catch (IllegalStateException e) {
      return true;
    }
  }

  private static boolean throwsArgument(Runnable r) {
    try {
      r.run();
      return false;
    } catch (IllegalArgumentException e) {
      return true;
    }
  }
}
