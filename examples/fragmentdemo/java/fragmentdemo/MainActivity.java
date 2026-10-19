// SPDX-License-Identifier: GPL-3.0-only
package fragmentdemo;

import picodroid.app.Activity;
import picodroid.app.Fragment;
import picodroid.app.FragmentFactory;
import picodroid.app.FragmentManager;
import picodroid.app.FragmentTransaction;
import picodroid.content.Intent;
import picodroid.lifecycle.DefaultLifecycleObserver;
import picodroid.lifecycle.Lifecycle;
import picodroid.lifecycle.LifecycleEventObserver;
import picodroid.lifecycle.LifecycleOwner;
import picodroid.lifecycle.LiveData;
import picodroid.lifecycle.MediatorLiveData;
import picodroid.lifecycle.MutableLiveData;
import picodroid.lifecycle.Observer;
import picodroid.lifecycle.Transformations;
import picodroid.lifecycle.ViewModel;
import picodroid.lifecycle.ViewModelProvider;
import picodroid.os.Bundle;
import picodroid.os.SystemClock;
import picodroid.util.Log;
import picodroid.view.View;
import picodroid.view.ViewGroup;
import picodroid.widget.FragmentStateAdapter;
import picodroid.widget.ViewPager2;

/**
 * Conformance driver for {@code picodroid.app.Fragment}: the callback order against the host's,
 * replace with a back stack, hide/show, detach/attach, {@code LiveData} against a fragment's view
 * lifecycle and a {@code ViewModel} shared through the host, a headless fragment, the refused
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

  /** The first instance's lifecycle and ViewModel, to look at once that instance is gone. */
  private static Lifecycle firstLifecycle;

  private static DemoViewModel firstModel;
  private static int modelsMade;
  private static int modelsCleared;

  /** What {@code postValue} delivered, a tick after it was posted. */
  private static String posted;

  /**
   * The Android shape: data the Activity and its fragments share, minus the views. Public with a
   * public no-argument constructor, which is what the default factory ({@code
   * ViewModelProvider.NewInstanceFactory}) constructs it through.
   */
  public static final class DemoViewModel extends ViewModel {
    final MutableLiveData<String> line = new MutableLiveData<>();

    public DemoViewModel() {
      modelsMade++;
    }

    @Override
    protected void onCleared() {
      modelsCleared++;
    }
  }

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
    // No FragmentFactory: the restore inside super.onCreate re-creates the saved fragments
    // through their public no-argument constructors, as Android's default factory does.
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
      T.check("the default factory made both", home != null && detail != null);
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
      stepLifecycle();
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

  private void stepLifecycle() {
    // LiveData bound to the fragment's view.
    DemoViewModel model = new ViewModelProvider(this).get(DemoViewModel.class);
    MutableLiveData<String> live = model.line;
    StringBuilder seen = new StringBuilder();
    Observer<String> observer = v -> seen.append(v).append(';');
    LifecycleOwner view = detail.getViewLifecycleOwner();
    T.check(
        "view lifecycle resumed with its fragment",
        view.getLifecycle().getCurrentState() == Lifecycle.State.RESUMED
            && detail.getViewLifecycleOwner() == view);
    live.observe(view, observer);
    live.observe(view, observer);
    T.check(
        "observing an empty LiveData delivers nothing",
        seen.length() == 0 && live.hasActiveObservers() && live.getValue() == null);
    live.setValue("a");
    T.check("setValue delivers inside the call", seen.toString().equals("a;"));
    StringBuilder late = new StringBuilder();
    live.observe(view, v -> late.append(v));
    T.check("a late observer gets the current value", late.toString().equals("a"));
    fm.beginTransaction().detach(detail).commitNow();
    T.check(
        "the view going takes its observers",
        view.getLifecycle().getCurrentState() == Lifecycle.State.DESTROYED && !live.hasObservers());
    live.setValue("b");
    T.check("nothing delivered to a destroyed view", seen.toString().equals("a;"));
    T.check("no view, no view lifecycle", throwsState(() -> detail.getViewLifecycleOwner()));
    fm.beginTransaction().attach(detail).commitNow();
    T.check("a new view has a new lifecycle", detail.getViewLifecycleOwner() != view);

    // An owner that stops and starts, driven by hand, with the observer kinds Android has.
    Lifecycle own = new Lifecycle();
    StringBuilder calls = new StringBuilder();
    own.addObserver(
        new DefaultLifecycleObserver() {
          @Override
          public void onCreate(LifecycleOwner owner) {
            calls.append(owner == own ? "c" : "?");
          }

          @Override
          public void onStart(LifecycleOwner owner) {
            calls.append('S');
          }

          @Override
          public void onStop(LifecycleOwner owner) {
            calls.append('s');
          }

          @Override
          public void onDestroy(LifecycleOwner owner) {
            calls.append('D');
          }
        });
    StringBuilder events = new StringBuilder();
    LifecycleEventObserver eventObserver =
        (source, event) -> events.append(event.name()).append(' ');
    own.addObserver(eventObserver);
    T.check("nothing replayed to an observer of an INITIALIZED owner", events.length() == 0);
    own.setCurrentState(Lifecycle.State.CREATED);
    T.check(
        "ON_CREATE reaches both observer kinds",
        calls.toString().equals("c") && events.toString().equals("ON_CREATE "));
    StringBuilder lateObs = new StringBuilder();
    own.addObserver(
        new DefaultLifecycleObserver() {
          @Override
          public void onCreate(LifecycleOwner owner) {
            lateObs.append('c');
          }

          @Override
          public void onResume(LifecycleOwner owner) {
            lateObs.append('R');
          }
        });
    T.check("a late observer is brought up to the current state", lateObs.toString().equals("c"));
    T.check(
        "Event.getTargetState / State.isAtLeast",
        Lifecycle.Event.ON_START.getTargetState() == Lifecycle.State.STARTED
            && Lifecycle.Event.ON_STOP.getTargetState() == Lifecycle.State.CREATED
            && Lifecycle.State.RESUMED.isAtLeast(Lifecycle.State.STARTED)
            && !Lifecycle.State.CREATED.isAtLeast(Lifecycle.State.STARTED)
            && Lifecycle.Event.downFrom(Lifecycle.State.RESUMED) == Lifecycle.Event.ON_PAUSE
            && Lifecycle.Event.upFrom(Lifecycle.State.RESUMED) == null);
    StringBuilder held = new StringBuilder();
    live.observe(own, v -> held.append(v));
    live.setValue("c");
    T.check("a stopped owner hears nothing", held.length() == 0 && !live.hasActiveObservers());
    own.setCurrentState(Lifecycle.State.STARTED);
    T.check("started: the value it missed", held.toString().equals("c"));
    live.setValue("d");
    own.setCurrentState(Lifecycle.State.CREATED);
    live.setValue("e");
    live.setValue("f");
    own.setCurrentState(Lifecycle.State.RESUMED);
    T.check("only the last of the values missed", held.toString().equals("cdf"));
    own.removeObserver(eventObserver);
    own.setCurrentState(Lifecycle.State.DESTROYED);
    T.check("a destroyed owner is forgotten", !live.hasObservers());
    T.check(
        "every step was dispatched in order",
        calls.toString().equals("cSsSsD")
            && lateObs.toString().equals("cR")
            && events.toString().equals("ON_CREATE ON_START ON_STOP ON_START ON_RESUME "));
    own.addObserver(eventObserver);
    T.check(
        "a destroyed Lifecycle replays nothing and stays destroyed",
        events.toString().equals("ON_CREATE ON_START ON_STOP ON_START ON_RESUME ")
            && own.getCurrentState() == Lifecycle.State.DESTROYED);
    Lifecycle fresh = new Lifecycle();
    fresh.addObserver(eventObserver);
    fresh.handleLifecycleEvent(Lifecycle.Event.ON_RESUME);
    fresh.setCurrentState(Lifecycle.State.DESTROYED);
    T.check(
        "handleLifecycleEvent walks the intermediate states",
        events.toString().endsWith("ON_CREATE ON_START ON_RESUME ON_PAUSE ON_STOP ON_DESTROY "));
    live.observe(own, v -> held.append(v));
    T.check("and cannot observe again", !live.hasObservers());

    // observeForever, removal from inside a callback, and postValue.
    MutableLiveData<String> other = new MutableLiveData<>();
    StringBuilder forever = new StringBuilder();
    Observer<String> once =
        new Observer<String>() {
          @Override
          public void onChanged(String v) {
            forever.append(v);
            other.removeObserver(this);
          }
        };
    other.observeForever(once);
    other.observeForever(v -> posted = v);
    other.setValue("y");
    T.check(
        "an observer that removes itself, and the one after it",
        forever.toString().equals("y") && "y".equals(posted) && other.hasObservers());
    other.postValue("p1");
    other.postValue("p2");
    // Derived LiveData: Transformations.map, a MediatorLiveData over two sources, switchMap.
    MutableLiveData<Integer> count = new MutableLiveData<>(2);
    LiveData<String> label = Transformations.map(count, n -> "n=" + n);
    T.check("map is idle until observed", label.getValue() == null && !count.hasObservers());
    StringBuilder mapped = new StringBuilder();
    Observer<String> mapObserver = v -> mapped.append(v).append(' ');
    label.observeForever(mapObserver);
    count.setValue(3);
    T.check("map follows its source", mapped.toString().equals("n=2 n=3 ") && count.hasObservers());
    label.removeObserver(mapObserver);
    T.check("an unobserved map lets go of its source", !count.hasObservers());
    MediatorLiveData<String> both = new MediatorLiveData<>();
    MutableLiveData<String> first = new MutableLiveData<>("a0");
    MutableLiveData<String> second = new MutableLiveData<>();
    both.addSource(first, v -> both.setValue("A:" + v));
    both.addSource(second, v -> both.setValue("B:" + v));
    StringBuilder merged = new StringBuilder();
    Observer<String> mergedObserver = v -> merged.append(v).append(' ');
    both.observeForever(mergedObserver);
    second.setValue("b1");
    first.setValue("a1");
    T.check("a mediator merges its sources", merged.toString().equals("A:a0 B:b1 A:a1 "));
    both.removeSource(first);
    first.setValue("a2");
    T.check("a removed source is silent", merged.toString().equals("A:a0 B:b1 A:a1 "));
    both.removeObserver(mergedObserver);
    second.setValue("b2");
    T.check(
        "an inactive mediator unplugs its sources",
        !second.hasObservers() && merged.toString().equals("A:a0 B:b1 A:a1 "));
    both.observeForever(mergedObserver);
    T.check(
        "a reactivated mediator catches up once", merged.toString().equals("A:a0 B:b1 A:a1 B:b2 "));
    both.removeObserver(mergedObserver);
    MutableLiveData<Boolean> pick = new MutableLiveData<>(true);
    MutableLiveData<String> left = new MutableLiveData<>("L");
    MutableLiveData<String> right = new MutableLiveData<>("R");
    LiveData<String> chosen = Transformations.switchMap(pick, p -> p ? left : right);
    StringBuilder switched = new StringBuilder();
    chosen.observeForever(v -> switched.append(v));
    pick.setValue(false);
    right.setValue("R2");
    left.setValue("L2");
    T.check(
        "switchMap follows the chosen source",
        switched.toString().equals("LRR2") && !left.hasObservers() && right.hasObservers());

    T.check("postValue waits for the main thread", "y".equals(posted));

    // The host's own lifecycle, and its ViewModels.
    firstLifecycle = getLifecycle();
    firstModel = model;
    T.check("the Activity is resumed", firstLifecycle.getCurrentState() == Lifecycle.State.RESUMED);
    T.check(
        "one ViewModel per owner and class",
        new ViewModelProvider(this).get(DemoViewModel.class) == model && modelsMade == 1);
    T.check(
        "another key, another instance",
        new ViewModelProvider(this).get("second", DemoViewModel.class) != model && modelsMade == 2);
    T.check(
        "a kept key is not made again",
        new ViewModelProvider(this, new ViewModelProvider.NewInstanceFactory())
                    .get("second", DemoViewModel.class)
                != model
            && modelsMade == 2);
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
    // A replace that evicts more fragments than one op implies (six added to the container)
    // used to overflow the transaction's fixed expansion buffers.
    HeadlessFragment[] many = new HeadlessFragment[6];
    for (int i = 0; i < many.length; i++) {
      many[i] = new HeadlessFragment();
      fm.beginTransaction().add(R.id.container, many[i]).commitNow();
    }
    boolean replacedMany;
    try {
      fm.beginTransaction()
          .replace(R.id.container, new HeadlessFragment(), "many")
          .addToBackStack(null)
          .commit();
      fm.executePendingTransactions();
      replacedMany = fm.findFragmentByTag("many") != null && !detail.isAdded();
    } catch (RuntimeException e) {
      T.log("replace of many threw " + e);
      replacedMany = false;
    }
    fm.popBackStackImmediate();
    for (HeadlessFragment h : many) {
      fm.beginTransaction().remove(h).commitNow();
    }
    T.check(
        "replace evicting six fragments",
        replacedMany && fm.findFragmentByTag("detail") == detail && detail.isAdded());
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
    // popBackStack(name, POP_BACK_STACK_INCLUSIVE) also consumes the same-named entries directly
    // beneath the one found, as Android does; it used to pop one.
    int base = fm.getBackStackEntryCount();
    for (String name : new String[] {"dup", "dup", "top"}) {
      fm.beginTransaction()
          .replace(R.id.container, new HeadlessFragment(), "named")
          .addToBackStack(name)
          .commit();
      fm.executePendingTransactions();
    }
    T.check("three named entries pushed", fm.getBackStackEntryCount() == base + 3);
    fm.popBackStackImmediate("dup", FragmentManager.POP_BACK_STACK_INCLUSIVE);
    T.check(
        "inclusive pop takes every same-named entry beneath",
        fm.getBackStackEntryCount() == base
            && fm.findFragmentByTag("detail") == detail
            && detail.isResumed());
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
    T.check("postValue delivered the last value posted", "p2".equals(posted));
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
        "the first host's lifecycle ended and its ViewModels were cleared",
        firstLifecycle.getCurrentState() == Lifecycle.State.DESTROYED && modelsCleared == 2);
    T.check(
        "a re-created host starts with a new ViewModel",
        new ViewModelProvider(this).get(DemoViewModel.class) != firstModel && modelsMade == 3);
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
    stepPager();
  }

  /** The items a pager shows, by stable id; removable from the front. */
  private final java.util.ArrayList<Long> pageIds = new java.util.ArrayList<Long>();

  private ViewPager2 pager;

  /**
   * A ViewPager2 over stable item ids: a negative setCurrentItem before the adapter is clamped (it
   * used to leave the pager blank for ever), and removing the item below the bound page re-binds
   * the page the position now carries (the old page used to stay, labelled with the new position).
   */
  private void stepPager() {
    pageIds.add(10L);
    pageIds.add(20L);
    pageIds.add(30L);
    pager = new ViewPager2(this);
    pager.setSize(200, 100);
    ((ViewGroup) findViewById(R.id.container)).addView(pager);
    pager.setCurrentItem(-3, false);
    pager.setAdapter(
        new FragmentStateAdapter(this) {
          @Override
          public Fragment createFragment(int position) {
            return new PageFragment(pageIds.get(position));
          }

          @Override
          public int getItemCount() {
            return pageIds.size();
          }

          @Override
          public long getItemId(int position) {
            return pageIds.get(position);
          }

          @Override
          public boolean containsItem(long itemId) {
            return pageIds.contains(itemId);
          }
        });
    T.later(250, () -> pagerBoundFirst());
  }

  private void pagerBoundFirst() {
    T.check(
        "negative setCurrentItem before the adapter lands on page 0",
        pager.getCurrentItem() == 0 && fm.findFragmentByTag("f10") != null);
    pager.setCurrentItem(1, false);
    T.later(250, () -> pagerOnSecond());
  }

  private void pagerOnSecond() {
    T.check("page 1 bound (item 20)", fm.findFragmentByTag("f20") != null);
    pageIds.remove(0);
    pager.getAdapter().notifyDataSetChanged();
    T.later(300, () -> pagerAfterRemoval());
  }

  private void pagerAfterRemoval() {
    T.check(
        "removing item 0 re-binds position 1 to the item it now carries (30)",
        pager.getCurrentItem() == 1
            && fm.findFragmentByTag("f20") == null
            && fm.findFragmentByTag("f30") != null);
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
