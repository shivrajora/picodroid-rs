// SPDX-License-Identifier: GPL-3.0-only
package picodroid.app;

import java.util.ArrayList;
import java.util.List;
import picodroid.concurrent.Executors;
import picodroid.os.Bundle;
import picodroid.util.Log;
import picodroid.view.View;
import picodroid.view.ViewGroup;

/**
 * Hosts an Activity's fragments, mirroring {@code androidx.fragment.app.FragmentManager}: obtain it
 * with {@link Activity#getSupportFragmentManager}. It runs {@link FragmentTransaction}s, walks each
 * fragment through its lifecycle as the host's own moves, keeps the back stack, and saves and
 * restores the whole set with the host's instance state.
 *
 * <p>Every fragment's state is the lowest of the host's state, the fragment's {@link
 * FragmentTransaction#setMaxLifecycle} cap, and {@link Fragment#CREATED} for one that is detached
 * or waiting in the back stack; a removed fragment outside the back stack is destroyed and
 * forgotten. Within a transaction fragments moving down go first, so a replaced page's widgets are
 * freed before the new page's are allocated. Views are appended to their container in the order
 * fragments reach {@link Fragment#VIEW_CREATED}.
 *
 * <p>Everything here runs on the main thread. Fragments come back after the host is re-created only
 * through a {@link FragmentFactory}; without one, the restore throws and the host starts with no
 * fragments.
 */
public class FragmentManager {
  /** Flag for {@link #popBackStack(String, int)}: pop the named entry too. */
  public static final int POP_BACK_STACK_INCLUSIVE = 1;

  /** Mirrors Android: told after every back stack push and pop. */
  public interface OnBackStackChangedListener {
    void onBackStackChanged();
  }

  private Activity mHost;
  private int mCurState = Fragment.INITIALIZING;

  /** Every fragment this manager knows, in the order it met them. */
  final ArrayList<Fragment> mActive = new ArrayList<>();

  /** The added fragments, in add order. */
  final ArrayList<Fragment> mAdded = new ArrayList<>();

  private final ArrayList<FragmentTransaction> mBackStack = new ArrayList<>();

  /** Commits and pop requests waiting to run, in order. */
  private final ArrayList<FragmentTransaction> mPending = new ArrayList<>();

  private ArrayList<OnBackStackChangedListener> mBackStackChangeListeners;
  private FragmentFactory mFactory;
  private int mNextWho = 1;
  private int mNextBackStackIndex;
  private boolean mStateSaved;
  private boolean mDestroyed;
  private boolean mExecuting;

  /** Posted once per commit; allocated once rather than per commit. */
  @SuppressWarnings("UnnecessaryLambda")
  private final Runnable mDrain = () -> execPendingActions();

  /** For {@code host}, which is at {@code hostState} (a {@link Fragment} state constant). */
  FragmentManager(Activity host, int hostState) {
    mHost = host;
    mCurState = hostState;
  }

  public FragmentTransaction beginTransaction() {
    return new FragmentTransaction(this);
  }

  /** Run every transaction committed so far. Returns whether there was one. */
  public boolean executePendingTransactions() {
    return execPendingActions();
  }

  /** The most recently added fragment in the container {@code id}, or {@code null}. */
  public Fragment findFragmentById(int id) {
    for (int i = mAdded.size() - 1; i >= 0; i--) {
      Fragment f = mAdded.get(i);
      if (f.mContainerId == id) {
        return f;
      }
    }
    for (int i = mActive.size() - 1; i >= 0; i--) {
      Fragment f = mActive.get(i);
      if (f.mContainerId == id) {
        return f;
      }
    }
    return null;
  }

  /** The most recently added fragment tagged {@code tag}, or {@code null}. */
  public Fragment findFragmentByTag(String tag) {
    if (tag == null) {
      return null;
    }
    for (int i = mAdded.size() - 1; i >= 0; i--) {
      Fragment f = mAdded.get(i);
      if (tag.equals(f.mTag)) {
        return f;
      }
    }
    for (int i = mActive.size() - 1; i >= 0; i--) {
      Fragment f = mActive.get(i);
      if (tag.equals(f.mTag)) {
        return f;
      }
    }
    return null;
  }

  /** A copy of the added fragments, in add order. */
  public List<Fragment> getFragments() {
    ArrayList<Fragment> out = new ArrayList<>(mAdded.size());
    for (int i = 0; i < mAdded.size(); i++) {
      out.add(mAdded.get(i));
    }
    return out;
  }

  /** Queue a pop of the top back stack entry, behind any commit already queued. */
  public void popBackStack() {
    enqueue(FragmentTransaction.popRequest(this, null, 0), false);
  }

  /**
   * Queue a pop of every entry above the one named {@code name} ({@code null}: the top entry), and
   * of that entry too with {@link #POP_BACK_STACK_INCLUSIVE}.
   */
  public void popBackStack(String name, int flags) {
    enqueue(FragmentTransaction.popRequest(this, name, flags), false);
  }

  /** Pop the top entry now. Returns whether there was one. */
  public boolean popBackStackImmediate() {
    return popBackStackImmediate(null, 0);
  }

  public boolean popBackStackImmediate(String name, int flags) {
    checkStateLoss();
    execPendingActions();
    ensureNotExecuting();
    mExecuting = true;
    try {
      return popBackStackState(name, flags);
    } finally {
      mExecuting = false;
    }
  }

  public int getBackStackEntryCount() {
    return mBackStack.size();
  }

  public void addOnBackStackChangedListener(OnBackStackChangedListener listener) {
    if (mBackStackChangeListeners == null) {
      mBackStackChangeListeners = new ArrayList<>();
    }
    mBackStackChangeListeners.add(listener);
  }

  public void removeOnBackStackChangedListener(OnBackStackChangedListener listener) {
    if (mBackStackChangeListeners != null) {
      mBackStackChangeListeners.remove(listener);
    }
  }

  /** Whether the host saved its state: transactions are refused until it restarts. */
  public boolean isStateSaved() {
    return mStateSaved;
  }

  public boolean isDestroyed() {
    return mDestroyed;
  }

  /** Install the factory that re-creates fragments after the host is destroyed and re-created. */
  public void setFragmentFactory(FragmentFactory factory) {
    mFactory = factory;
  }

  public FragmentFactory getFragmentFactory() {
    if (mFactory == null) {
      mFactory = new FragmentFactory();
    }
    return mFactory;
  }

  /**
   * What {@code fragment} would save now, as a Bundle for {@link Fragment#setInitialSavedState}
   * (Android returns a {@code SavedState}); {@code null} when it has nothing to save.
   */
  public Bundle saveFragmentInstanceState(Fragment fragment) {
    if (fragment.mFragmentManager != this) {
      throw new IllegalStateException(
          "Fragment " + fragment + " is not currently in the FragmentManager");
    }
    return fragment.mState > Fragment.INITIALIZING ? saveState(fragment) : null;
  }

  /** Mirrors Android: store a reference to {@code fragment} in {@code bundle} under {@code key}. */
  public void putFragment(Bundle bundle, String key, Fragment fragment) {
    if (fragment.mFragmentManager != this) {
      throw new IllegalStateException(
          "Fragment " + fragment + " is not currently in the FragmentManager");
    }
    bundle.putInt(key, fragment.mWho);
  }

  /**
   * Mirrors Android: the fragment {@link #putFragment} stored under {@code key}, or {@code null}.
   */
  public Fragment getFragment(Bundle bundle, String key) {
    int who = bundle.getInt(key, 0);
    if (who == 0) {
      return null;
    }
    Fragment f = findActiveByWho(who);
    if (f == null) {
      throw new IllegalStateException("Fragment no longer exists for key " + key + ": id " + who);
    }
    return f;
  }

  // ── Host lifecycle, from Activity ───────────────────────────────────────

  void dispatchCreate() {
    mStateSaved = false;
    dispatchStateChange(Fragment.CREATED);
  }

  void dispatchStart() {
    mStateSaved = false;
    dispatchStateChange(Fragment.STARTED);
  }

  void dispatchResume() {
    mStateSaved = false;
    dispatchStateChange(Fragment.RESUMED);
  }

  void dispatchPause() {
    dispatchStateChange(Fragment.STARTED);
  }

  void dispatchStop() {
    dispatchStateChange(Fragment.VIEW_CREATED);
  }

  void dispatchDestroy() {
    mDestroyed = true;
    execPendingActions();
    dispatchStateChange(Fragment.INITIALIZING);
    mActive.clear();
    mAdded.clear();
    mBackStack.clear();
    mPending.clear();
    mHost = null;
  }

  private void dispatchStateChange(int state) {
    mCurState = state;
    mExecuting = true;
    try {
      moveAllToExpectedState();
    } finally {
      mExecuting = false;
    }
    execPendingActions();
  }

  /**
   * A fragment the manager can place again. One added without a container whose view someone else
   * placed (a {@code ViewPager2} page) is that owner's to save and re-create: restored here it
   * would get a view nobody attaches.
   */
  static boolean isSavable(Fragment f) {
    return f.mContainerId != 0 || f.mView == null;
  }

  /** The host's fragment state as a Bundle, or {@code null} when there is nothing to keep. */
  Bundle saveAllState() {
    execPendingActions();
    mStateSaved = true;
    Bundle out = new Bundle();
    int n = 0;
    for (int i = 0; i < mActive.size(); i++) {
      Fragment f = mActive.get(i);
      if (!isSavable(f)) {
        continue;
      }
      Bundle fs = new Bundle();
      fs.putString("cls", f.getClass().getName());
      fs.putInt("who", f.mWho);
      fs.putInt("cid", f.mContainerId);
      if (f.mTag != null) {
        fs.putString("tag", f.mTag);
      }
      fs.putInt("flags", f.mFlags);
      fs.putInt("max", f.mMaxState);
      if (f.mArguments != null) {
        fs.putBundle("args", f.mArguments);
      }
      Bundle state = f.mState > Fragment.INITIALIZING ? saveState(f) : f.mSavedFragmentState;
      if (state != null) {
        fs.putBundle("state", state);
      }
      out.putBundle("f" + n, fs);
      n++;
    }
    if (n == 0) {
      return null;
    }
    out.putInt("n", n);
    int[] added = new int[mAdded.size()];
    int count = 0;
    for (int i = 0; i < added.length; i++) {
      Fragment f = mAdded.get(i);
      if (isSavable(f)) {
        added[count++] = f.mWho;
      }
    }
    int[] addedWhos = new int[count];
    System.arraycopy(added, 0, addedWhos, 0, count);
    out.putIntArray("added", addedWhos);
    int depth = 0;
    for (int k = 0; k < mBackStack.size(); k++) {
      FragmentTransaction t = mBackStack.get(k);
      if (t.namesUnsavable()) {
        break; // the stack above an entry that cannot come back is lost with it
      }
      out.putBundle("b" + k, t.saveState());
      depth++;
    }
    if (depth > 0) {
      out.putInt("bs", depth);
    }
    return out;
  }

  /**
   * Re-create the fragments {@link #saveAllState} recorded, at INITIALIZING, through the factory.
   * Without one nothing is restored, with a warning: Android's default factory reflects on the
   * class name, and there is no reflection here.
   */
  void restoreSaveState(Bundle state) {
    int n = state.getInt("n", 0);
    if (mFactory == null) {
      Log.w(
          "FragmentManager",
          n
              + " saved fragment(s) not restored: no FragmentFactory (setFragmentFactory before"
              + " super.onCreate)");
      return;
    }
    for (int i = 0; i < n; i++) {
      Bundle fs = state.getBundle("f" + i);
      if (fs == null) {
        continue;
      }
      String cls = fs.getString("cls");
      Fragment f = getFragmentFactory().instantiate(cls);
      if (f == null) {
        throw new IllegalStateException("FragmentFactory returned null for " + cls);
      }
      f.mWho = fs.getInt("who", 0);
      if (f.mWho >= mNextWho) {
        mNextWho = f.mWho + 1;
      }
      f.mContainerId = fs.getInt("cid", 0);
      f.mTag = fs.getString("tag");
      f.mFlags = fs.getInt("flags", 0);
      f.mMaxState = fs.getInt("max", Fragment.RESUMED);
      f.mArguments = fs.getBundle("args");
      f.mSavedFragmentState = fs.getBundle("state");
      f.mFragmentManager = this;
      mActive.add(f);
    }
    int[] added = state.getIntArray("added");
    if (added != null) {
      for (int i = 0; i < added.length; i++) {
        Fragment f = findActiveByWho(added[i]);
        if (f != null) {
          mAdded.add(f);
        }
      }
    }
    int depth = state.getInt("bs", 0);
    for (int k = 0; k < depth; k++) {
      Bundle b = state.getBundle("b" + k);
      FragmentTransaction t = b == null ? null : FragmentTransaction.restoreState(this, b);
      if (t == null) {
        break; // an entry naming a fragment that did not come back, and everything above it
      }
      t.bumpNesting(1);
      mBackStack.add(t);
    }
  }

  // ── Transactions ────────────────────────────────────────────────────────

  int enqueue(FragmentTransaction t, boolean allowStateLoss) {
    if (!allowStateLoss) {
      checkStateLoss();
    }
    if (mDestroyed || mHost == null) {
      if (allowStateLoss) {
        return -1;
      }
      throw new IllegalStateException("FragmentManager has been destroyed");
    }
    mPending.add(t);
    // One post per commit and no "posted" flag: the main queue drops a post when full, and the
    // next commit, lifecycle move or executePendingTransactions() drains in order anyway.
    Executors.mainExecutor().execute(mDrain);
    return t.isAddingToBackStack() ? mNextBackStackIndex++ : -1;
  }

  void execSingle(FragmentTransaction t, boolean allowStateLoss) {
    if (!allowStateLoss) {
      checkStateLoss();
    }
    if (mDestroyed || mHost == null) {
      if (allowStateLoss) {
        return;
      }
      throw new IllegalStateException("FragmentManager has been destroyed");
    }
    ensureNotExecuting();
    mExecuting = true;
    try {
      executeRecord(t);
    } finally {
      mExecuting = false;
    }
  }

  void checkStateLoss() {
    if (mStateSaved) {
      throw new IllegalStateException("Can not perform this action after onSaveInstanceState");
    }
  }

  private void ensureNotExecuting() {
    if (mExecuting) {
      throw new IllegalStateException("FragmentManager is already executing transactions");
    }
  }

  private boolean execPendingActions() {
    if (mPending.isEmpty()) {
      return false;
    }
    ensureNotExecuting();
    mExecuting = true;
    try {
      while (!mPending.isEmpty()) {
        FragmentTransaction t = mPending.remove(0);
        if (t.isPop()) {
          popBackStackState(t.name(), t.popFlags());
        } else {
          executeRecord(t);
        }
      }
    } finally {
      mExecuting = false;
    }
    return true;
  }

  private void executeRecord(FragmentTransaction t) {
    t.executeOps();
    if (t.isAddingToBackStack()) {
      t.bumpNesting(1);
      mBackStack.add(t);
    }
    moveAllToExpectedState();
    if (t.isAddingToBackStack()) {
      reportBackStackChanged();
    }
  }

  private boolean popBackStackState(String name, int flags) {
    int size = mBackStack.size();
    if (size == 0) {
      return false;
    }
    int index;
    if (name == null) {
      index = size - 1;
    } else {
      index = -1;
      for (int i = size - 1; i >= 0; i--) {
        if (name.equals(mBackStack.get(i).name())) {
          index = i;
          break;
        }
      }
      if (index < 0) {
        return false;
      }
      if ((flags & POP_BACK_STACK_INCLUSIVE) == 0) {
        index++;
      }
      if (index >= size) {
        return false;
      }
    }
    for (int i = size - 1; i >= index; i--) {
      FragmentTransaction t = mBackStack.remove(i);
      t.executePopOps();
      t.bumpNesting(-1);
    }
    moveAllToExpectedState();
    reportBackStackChanged();
    return true;
  }

  private void reportBackStackChanged() {
    ArrayList<OnBackStackChangedListener> listeners = mBackStackChangeListeners;
    if (listeners == null) {
      return;
    }
    for (int i = 0; i < listeners.size(); i++) {
      listeners.get(i).onBackStackChanged();
    }
  }

  // ── Fragment bookkeeping, from FragmentTransaction ──────────────────────

  Fragment findActiveByWho(int who) {
    for (int i = 0; i < mActive.size(); i++) {
      Fragment f = mActive.get(i);
      if (f.mWho == who) {
        return f;
      }
    }
    return null;
  }

  void addFragment(Fragment f) {
    if (f.mFragmentManager != null && f.mFragmentManager != this) {
      throw new IllegalStateException("Fragment " + f + " belongs to another FragmentManager");
    }
    if (f.mWho == 0) {
      f.mWho = mNextWho++;
    }
    f.mFragmentManager = this;
    if (!mActive.contains(f)) {
      mActive.add(f);
    }
    if ((f.mFlags & Fragment.F_DETACHED) == 0) {
      if ((f.mFlags & Fragment.F_ADDED) != 0) {
        throw new IllegalStateException("Fragment already added: " + f);
      }
      mAdded.add(f);
      f.mFlags = (f.mFlags | Fragment.F_ADDED) & ~Fragment.F_REMOVING;
    }
  }

  void removeFragment(Fragment f) {
    f.mFlags |= Fragment.F_REMOVING;
    if ((f.mFlags & Fragment.F_ADDED) != 0) {
      mAdded.remove(f);
      f.mFlags &= ~Fragment.F_ADDED;
    }
  }

  void hideFragment(Fragment f) {
    if (!f.isHidden()) {
      f.mFlags |= Fragment.F_HIDDEN;
      if (f.mView != null) {
        f.mView.setVisibility(View.GONE);
      }
      if (f.mState >= Fragment.ATTACHED) {
        f.onHiddenChanged(true);
      }
    }
  }

  void showFragment(Fragment f) {
    if (f.isHidden()) {
      f.mFlags &= ~Fragment.F_HIDDEN;
      if (f.mView != null) {
        f.mView.setVisibility(View.VISIBLE);
      }
      if (f.mState >= Fragment.ATTACHED) {
        f.onHiddenChanged(false);
      }
    }
  }

  void detachFragment(Fragment f) {
    if (!f.isDetached()) {
      f.mFlags |= Fragment.F_DETACHED;
      if ((f.mFlags & Fragment.F_ADDED) != 0) {
        mAdded.remove(f);
        f.mFlags &= ~Fragment.F_ADDED;
      }
    }
  }

  void attachFragment(Fragment f) {
    if (f.isDetached()) {
      f.mFlags &= ~Fragment.F_DETACHED;
      if ((f.mFlags & Fragment.F_ADDED) == 0) {
        if (mAdded.contains(f)) {
          throw new IllegalStateException("Fragment already added: " + f);
        }
        mAdded.add(f);
        f.mFlags |= Fragment.F_ADDED;
      }
    }
  }

  void setMaxLifecycle(Fragment f, int state) {
    if (f.mFragmentManager != this) {
      throw new IllegalStateException(
          "Cannot setMaxLifecycle for Fragment not attached to FragmentManager " + this);
    }
    f.mMaxState = state;
  }

  // ── The state machine ───────────────────────────────────────────────────

  private int expectedState(Fragment f) {
    int s = Math.min(mCurState, f.mMaxState);
    if ((f.mFlags & Fragment.F_ADDED) == 0) {
      boolean gone = (f.mFlags & Fragment.F_REMOVING) != 0 && !f.isInBackStack();
      s = Math.min(s, gone ? Fragment.INITIALIZING : Fragment.CREATED);
    }
    return s;
  }

  /**
   * Fragments moving down first, newest first, then fragments moving up in the order they were met:
   * an outgoing page's widgets are freed before the incoming page's are allocated.
   */
  private void moveAllToExpectedState() {
    for (int i = mActive.size() - 1; i >= 0; i--) {
      Fragment f = mActive.get(i);
      int target = expectedState(f);
      if (target < f.mState) {
        moveToState(f, target);
      }
      if (f.mFragmentManager == null) {
        mActive.remove(i);
      }
    }
    for (int i = 0; i < mActive.size(); i++) {
      Fragment f = mActive.get(i);
      int target = expectedState(f);
      if (target > f.mState) {
        moveToState(f, target);
      }
    }
  }

  /** One step at a time; a callback that throws leaves the fragment at the step it reached. */
  private void moveToState(Fragment f, int target) {
    while (f.mState < target) {
      switch (f.mState) {
        case Fragment.INITIALIZING:
          f.mHost = mHost;
          f.mFragmentManager = this;
          f.onAttach(mHost);
          f.mState = Fragment.ATTACHED;
          break;
        case Fragment.ATTACHED:
          f.onCreate(f.mSavedFragmentState);
          f.mState = Fragment.CREATED;
          break;
        case Fragment.CREATED:
          createView(f);
          f.mState = Fragment.VIEW_CREATED;
          break;
        case Fragment.VIEW_CREATED:
          f.onStart();
          f.mState = Fragment.STARTED;
          break;
        case Fragment.STARTED:
          f.onResume();
          f.mState = Fragment.RESUMED;
          break;
        default:
          return;
      }
    }
    while (f.mState > target) {
      switch (f.mState) {
        case Fragment.RESUMED:
          f.onPause();
          f.mState = Fragment.STARTED;
          break;
        case Fragment.STARTED:
          f.onStop();
          f.mState = Fragment.VIEW_CREATED;
          break;
        case Fragment.VIEW_CREATED:
          destroyView(f);
          f.mState = Fragment.CREATED;
          break;
        case Fragment.CREATED:
          f.onDestroy();
          f.mState = Fragment.ATTACHED;
          break;
        case Fragment.ATTACHED:
          f.onDetach();
          f.mHost = null;
          f.mFragmentManager = null;
          f.mState = Fragment.INITIALIZING;
          break;
        default:
          return;
      }
    }
  }

  private void createView(Fragment f) {
    ViewGroup container = null;
    if (f.mContainerId != 0) {
      View found = mHost.findViewById(f.mContainerId);
      if (found == null) {
        throw new IllegalArgumentException(
            "No view found for id " + f.mContainerId + " for fragment " + f);
      }
      if (!(found instanceof ViewGroup)) {
        throw new IllegalStateException(
            "View " + found + " for id " + f.mContainerId + " is not a ViewGroup");
      }
      container = (ViewGroup) found;
    }
    f.mContainer = container;
    Bundle saved = f.mSavedFragmentState;
    View view = f.onCreateView(mHost.getLayoutInflater(), container, saved);
    f.mView = view;
    if (view != null) {
      if (container != null) {
        // An inflated view carries its layout_* size in its LayoutParams; the two-argument add
        // is what applies them (Android's one-argument add reads them off the view).
        ViewGroup.LayoutParams lp = view.getLayoutParams();
        if (lp != null) {
          container.addView(view, lp);
        } else {
          container.addView(view);
        }
      }
      if (f.isHidden()) {
        view.setVisibility(View.GONE);
      }
      f.onViewCreated(view, saved);
      f.onViewStateRestored(saved);
    }
    f.mSavedFragmentState = null;
  }

  /** {@code onDestroyView} on the live tree, then free it: a removed view cannot be re-added. */
  private void destroyView(Fragment f) {
    f.onDestroyView();
    View view = f.mView;
    if (view != null) {
      ViewGroup parent = (ViewGroup) view.getParent();
      if (parent != null) {
        parent.removeView(view);
      } else {
        view.close();
      }
    }
    f.mView = null;
    f.mContainer = null;
  }

  private static Bundle saveState(Fragment f) {
    Bundle out = new Bundle();
    f.onSaveInstanceState(out);
    return out.isEmpty() ? null : out;
  }
}
