// SPDX-License-Identifier: GPL-3.0-only
package picodroid.app;

import picodroid.os.Bundle;

/**
 * A set of fragment operations applied together, mirroring {@code
 * androidx.fragment.app.FragmentTransaction}. Obtain one from {@link
 * FragmentManager#beginTransaction}, chain operations, then {@link #commit} (applied on a later
 * main-thread tick, as on Android) or {@link #commitNow}. A transaction committed with {@link
 * #addToBackStack} is also the record the manager keeps to reverse it on {@link
 * FragmentManager#popBackStack}: {@code add} is undone by a remove, {@code replace} by removing the
 * new fragment and adding the replaced ones back (their views built again), {@code hide} by a show,
 * {@code detach} by an attach.
 *
 * <p>Not provided: {@code setCustomAnimations}, {@code setTransition}, shared elements, {@code
 * runOnCommit}, {@code setPrimaryNavigationFragment}. {@link #setReorderingAllowed} is accepted and
 * ignored.
 */
public class FragmentTransaction {
  static final int OP_ADD = 1;
  static final int OP_REPLACE = 2;
  static final int OP_REMOVE = 3;
  static final int OP_HIDE = 4;
  static final int OP_SHOW = 5;
  static final int OP_DETACH = 6;
  static final int OP_ATTACH = 7;
  static final int OP_SET_MAX_LIFECYCLE = 10;

  private final FragmentManager mManager;

  /** Per op, three ints: the command, its argument (container id or max state), the old value. */
  private int[] mInts = new int[12];

  private Fragment[] mFrags = new Fragment[4];
  private int mOps;
  private boolean mAddToBackStack;
  private boolean mCommitted;

  /** A queued {@code popBackStack} request rather than a commit; {@link #mName} is its name. */
  private boolean mIsPop;

  private String mName;
  private int mPopFlags;

  FragmentTransaction(FragmentManager manager) {
    mManager = manager;
  }

  /** A {@link FragmentManager#popBackStack(String, int)} request, queued behind earlier commits. */
  static FragmentTransaction popRequest(FragmentManager manager, String name, int flags) {
    FragmentTransaction t = new FragmentTransaction(manager);
    t.mIsPop = true;
    t.mName = name;
    t.mPopFlags = flags;
    return t;
  }

  boolean isPop() {
    return mIsPop;
  }

  String name() {
    return mName;
  }

  int popFlags() {
    return mPopFlags;
  }

  boolean isAddingToBackStack() {
    return mAddToBackStack;
  }

  private void op(int cmd, Fragment f, int arg, int old) {
    if (f == null) {
      throw new IllegalArgumentException("fragment is null");
    }
    if (mOps == mFrags.length) {
      int[] ints = new int[mInts.length * 2];
      System.arraycopy(mInts, 0, ints, 0, mInts.length);
      mInts = ints;
      Fragment[] frags = new Fragment[mFrags.length * 2];
      System.arraycopy(mFrags, 0, frags, 0, mFrags.length);
      mFrags = frags;
    }
    int at = mOps * 3;
    mInts[at] = cmd;
    mInts[at + 1] = arg;
    mInts[at + 2] = old;
    mFrags[mOps] = f;
    mOps++;
  }

  private void doAddOp(int containerViewId, Fragment f, String tag, int cmd) {
    if (f == null) {
      throw new IllegalArgumentException("fragment is null");
    }
    if (tag != null) {
      if (f.mTag != null && !tag.equals(f.mTag)) {
        throw new IllegalStateException(
            "Can't change tag of fragment " + f + ": was " + f.mTag + " now " + tag);
      }
      f.mTag = tag;
    }
    if (containerViewId != 0) {
      if (f.mContainerId != 0 && f.mContainerId != containerViewId) {
        throw new IllegalStateException(
            "Can't change container ID of fragment "
                + f
                + ": was "
                + f.mContainerId
                + " now "
                + containerViewId);
      }
      f.mContainerId = containerViewId;
    }
    op(cmd, f, containerViewId, 0);
  }

  /** Add {@code fragment} to the container view {@code containerViewId} of the host. */
  public FragmentTransaction add(int containerViewId, Fragment fragment) {
    doAddOp(containerViewId, fragment, null, OP_ADD);
    return this;
  }

  public FragmentTransaction add(int containerViewId, Fragment fragment, String tag) {
    doAddOp(containerViewId, fragment, tag, OP_ADD);
    return this;
  }

  /**
   * Add {@code fragment} without a container: its {@code onCreateView} gets a {@code null} parent
   * and whoever owns the fragment places the view (a {@code ViewPager2} does), or it has none.
   */
  public FragmentTransaction add(Fragment fragment, String tag) {
    doAddOp(0, fragment, tag, OP_ADD);
    return this;
  }

  /** Remove every fragment in {@code containerViewId}, then add {@code fragment} there. */
  public FragmentTransaction replace(int containerViewId, Fragment fragment) {
    return replace(containerViewId, fragment, null);
  }

  public FragmentTransaction replace(int containerViewId, Fragment fragment, String tag) {
    if (containerViewId == 0) {
      throw new IllegalArgumentException("Must use non-zero containerViewId");
    }
    doAddOp(containerViewId, fragment, tag, OP_REPLACE);
    return this;
  }

  /** Remove {@code fragment}: its view is freed and, unless in the back stack, it is destroyed. */
  public FragmentTransaction remove(Fragment fragment) {
    op(OP_REMOVE, fragment, 0, 0);
    return this;
  }

  /** Hide {@code fragment}'s view ({@code GONE}); the fragment stays added and resumed. */
  public FragmentTransaction hide(Fragment fragment) {
    op(OP_HIDE, fragment, 0, 0);
    return this;
  }

  public FragmentTransaction show(Fragment fragment) {
    op(OP_SHOW, fragment, 0, 0);
    return this;
  }

  /** Free {@code fragment}'s view but keep the instance in the manager, at {@code CREATED}. */
  public FragmentTransaction detach(Fragment fragment) {
    op(OP_DETACH, fragment, 0, 0);
    return this;
  }

  /** Bring a detached fragment back: its view is built again. */
  public FragmentTransaction attach(Fragment fragment) {
    op(OP_ATTACH, fragment, 0, 0);
    return this;
  }

  /**
   * Cap {@code fragment}'s state at {@link Fragment#CREATED}, {@link Fragment#STARTED} or {@link
   * Fragment#RESUMED} (Android takes a {@code Lifecycle.State}). A cap of {@code CREATED} frees the
   * view; raising it later builds the view again.
   */
  public FragmentTransaction setMaxLifecycle(Fragment fragment, int state) {
    if (state != Fragment.CREATED && state != Fragment.STARTED && state != Fragment.RESUMED) {
      throw new IllegalArgumentException(
          "setMaxLifecycle: state must be CREATED, STARTED or RESUMED, not " + state);
    }
    op(OP_SET_MAX_LIFECYCLE, fragment, state, 0);
    return this;
  }

  /** Keep this transaction so {@link FragmentManager#popBackStack} can reverse it. */
  public FragmentTransaction addToBackStack(String name) {
    mAddToBackStack = true;
    mName = name;
    return this;
  }

  /** Accepted for source compatibility; transactions here always run in order. */
  public FragmentTransaction setReorderingAllowed(boolean allowed) {
    return this;
  }

  public boolean isEmpty() {
    return mOps == 0;
  }

  /**
   * Schedule this transaction on the main thread, as Android does; it runs on a later tick, or
   * sooner when the host's lifecycle moves or {@link FragmentManager#executePendingTransactions} is
   * called. Refused after the host saved its state. Returns the back stack entry's index, or -1.
   */
  public int commit() {
    return commitInternal(false);
  }

  /** {@link #commit} that skips the state-saved check: a queued transaction may then be lost. */
  public int commitAllowingStateLoss() {
    return commitInternal(true);
  }

  private int commitInternal(boolean allowStateLoss) {
    if (mCommitted) {
      throw new IllegalStateException("commit already called");
    }
    mCommitted = true;
    return mManager.enqueue(this, allowStateLoss);
  }

  /** Run this transaction now. Not for a transaction added to the back stack, as on Android. */
  public void commitNow() {
    commitNowInternal(false);
  }

  public void commitNowAllowingStateLoss() {
    commitNowInternal(true);
  }

  private void commitNowInternal(boolean allowStateLoss) {
    if (mAddToBackStack) {
      throw new IllegalStateException("This transaction is already being added to the back stack");
    }
    if (mCommitted) {
      throw new IllegalStateException("commit already called");
    }
    mCommitted = true;
    mManager.execSingle(this, allowStateLoss);
  }

  // ── Execution, by the manager ───────────────────────────────────────────

  /**
   * Apply the ops in order. A {@code replace} becomes the removes it implied plus its add, and the
   * expanded list replaces the recorded one so a pop reverses exactly what ran.
   */
  void executeOps() {
    int[] ints = new int[mOps * 6 + 12];
    Fragment[] frags = new Fragment[mOps * 2 + 4];
    int n = 0;
    for (int i = 0; i < mOps; i++) {
      int cmd = mInts[i * 3];
      int arg = mInts[i * 3 + 1];
      Fragment f = mFrags[i];
      switch (cmd) {
        case OP_REPLACE:
          for (int j = 0; j < mManager.mAdded.size(); ) {
            Fragment a = mManager.mAdded.get(j);
            if (a.mContainerId == arg && a != f) {
              mManager.removeFragment(a);
              ints[n * 3] = OP_REMOVE;
              ints[n * 3 + 1] = 0;
              frags[n++] = a;
            } else {
              j++;
            }
          }
          if ((f.mFlags & Fragment.F_ADDED) == 0) {
            mManager.addFragment(f);
            ints[n * 3] = OP_ADD;
            ints[n * 3 + 1] = arg;
            frags[n++] = f;
          }
          break;
        case OP_ADD:
          mManager.addFragment(f);
          break;
        case OP_REMOVE:
          mManager.removeFragment(f);
          break;
        case OP_HIDE:
          mManager.hideFragment(f);
          break;
        case OP_SHOW:
          mManager.showFragment(f);
          break;
        case OP_DETACH:
          mManager.detachFragment(f);
          break;
        case OP_ATTACH:
          mManager.attachFragment(f);
          break;
        case OP_SET_MAX_LIFECYCLE:
          ints[n * 3 + 2] = f.mMaxState;
          mManager.setMaxLifecycle(f, arg);
          break;
        default:
          throw new IllegalStateException("Unknown cmd: " + cmd);
      }
      if (cmd != OP_REPLACE) {
        ints[n * 3] = cmd;
        ints[n * 3 + 1] = arg;
        frags[n++] = f;
      }
    }
    mInts = ints;
    mFrags = frags;
    mOps = n;
  }

  /** Reverse the ops, last first. */
  void executePopOps() {
    for (int i = mOps - 1; i >= 0; i--) {
      int cmd = mInts[i * 3];
      Fragment f = mFrags[i];
      switch (cmd) {
        case OP_ADD:
          mManager.removeFragment(f);
          break;
        case OP_REMOVE:
          mManager.addFragment(f);
          break;
        case OP_HIDE:
          mManager.showFragment(f);
          break;
        case OP_SHOW:
          mManager.hideFragment(f);
          break;
        case OP_DETACH:
          mManager.attachFragment(f);
          break;
        case OP_ATTACH:
          mManager.detachFragment(f);
          break;
        case OP_SET_MAX_LIFECYCLE:
          mManager.setMaxLifecycle(f, mInts[i * 3 + 2]);
          break;
        default:
          throw new IllegalStateException("Unknown cmd: " + cmd);
      }
    }
  }

  /** Count this record in or out of every touched fragment's back stack membership. */
  void bumpNesting(int by) {
    for (int i = 0; i < mOps; i++) {
      mFrags[i].mBackStackNesting += by;
    }
  }

  /** Whether this record touches a fragment the manager cannot save (see {@code isSavable}). */
  boolean namesUnsavable() {
    for (int i = 0; i < mOps; i++) {
      if (!FragmentManager.isSavable(mFrags[i])) {
        return true;
      }
    }
    return false;
  }

  Bundle saveState() {
    Bundle b = new Bundle();
    if (mName != null) {
      b.putString("name", mName);
    }
    int[] ops = new int[mOps * 4];
    for (int i = 0; i < mOps; i++) {
      ops[i * 4] = mInts[i * 3];
      ops[i * 4 + 1] = mFrags[i].mWho;
      ops[i * 4 + 2] = mInts[i * 3 + 1];
      ops[i * 4 + 3] = mInts[i * 3 + 2];
    }
    b.putIntArray("ops", ops);
    return b;
  }

  static FragmentTransaction restoreState(FragmentManager manager, Bundle b) {
    FragmentTransaction t = new FragmentTransaction(manager);
    t.mName = b.getString("name");
    t.mAddToBackStack = true;
    t.mCommitted = true;
    int[] ops = b.getIntArray("ops");
    if (ops != null) {
      for (int i = 0; i + 3 < ops.length; i += 4) {
        Fragment f = manager.findActiveByWho(ops[i + 1]);
        if (f == null) {
          return null; // names a fragment that did not come back
        }
        t.op(ops[i], f, ops[i + 2], ops[i + 3]);
      }
    }
    return t;
  }
}
