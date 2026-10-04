// SPDX-License-Identifier: GPL-3.0-only
package picodroid.app;

import picodroid.content.Context;
import picodroid.content.Intent;
import picodroid.content.res.Resources;
import picodroid.lifecycle.Lifecycle;
import picodroid.lifecycle.LifecycleOwner;
import picodroid.os.Bundle;
import picodroid.view.LayoutInflater;
import picodroid.view.View;
import picodroid.view.ViewGroup;

/**
 * A reusable portion of an Activity's UI with a lifecycle of its own, mirroring {@code
 * androidx.fragment.app.Fragment}. A fragment defines and manages its own view tree, is hosted by
 * an {@link Activity}, and is added, replaced, removed, hidden or shown through the host's {@link
 * FragmentManager}:
 *
 * <pre>{@code
 * getSupportFragmentManager()
 *     .beginTransaction()
 *     .replace(R.id.container, new DetailFragment())
 *     .addToBackStack(null)
 *     .commit();
 * }</pre>
 *
 * <p>The callbacks arrive in Android's order: {@link #onAttach}, {@link #onCreate}, {@link
 * #onCreateView}, {@link #onViewCreated}, {@link #onViewStateRestored}, {@link #onStart}, {@link
 * #onResume}; then {@link #onPause}, {@link #onStop}, {@link #onSaveInstanceState} (when the host
 * saves), {@link #onDestroyView}, {@link #onDestroy}, {@link #onDetach}. The host's own lifecycle
 * drives them: its fragments are created after its {@code onCreate}, started inside its {@code
 * onStart}, resumed after its {@code onResume}, and paused, stopped and destroyed before the
 * matching host callback.
 *
 * <p>Two things differ from Android because an embedded panel cannot afford detached view trees. A
 * view a fragment gives up in {@link #onDestroyView} is freed at once (its LVGL widgets with it),
 * so every field holding one of its children must be treated as dead afterwards; the next {@link
 * #onCreateView} builds a fresh tree. And there is no reflection: after the host Activity is
 * destroyed and re-created, its fragments come back only through a {@link FragmentFactory} the app
 * installed with {@link FragmentManager#setFragmentFactory} before {@code super.onCreate}.
 *
 * <p>Lifecycle states are {@code int}s here ({@link #CREATED}, {@link #STARTED}, {@link #RESUMED})
 * rather than a {@code Lifecycle.State} enum; {@link FragmentTransaction#setMaxLifecycle} takes
 * them. Not provided: nested fragments ({@code getChildFragmentManager}), {@code
 * startActivityForResult} on a fragment (use the Activity's), transitions and animations, {@code
 * setRetainInstance} and menus. A fragment shares a {@link picodroid.lifecycle.ViewModel} with its
 * host through {@code new ViewModelProvider(requireActivity())} and observes with {@link
 * #getViewLifecycleOwner}; it is not a {@code LifecycleOwner} or a {@code ViewModelStoreOwner}
 * itself.
 *
 * <p>An override that skips {@code super} is tolerated, as it is on {@link Activity}: the base
 * callbacks are empty and nothing throws Android's {@code SuperNotCalledException}. Call it anyway;
 * code that does not would crash at the first attach on Android.
 */
public class Fragment {
  /** Not attached to any host. The first state, and the last after {@link #onDetach}. */
  public static final int INITIALIZING = 0;

  /** {@link #onAttach} has run. */
  public static final int ATTACHED = 1;

  /**
   * {@link #onCreate} has run; the fragment has no view. The lowest {@link
   * FragmentTransaction#setMaxLifecycle} target, and where a fragment in the back stack waits.
   */
  public static final int CREATED = 2;

  /** The view exists but the fragment is stopped (the host is stopped, or just started). */
  public static final int VIEW_CREATED = 3;

  /** {@link #onStart} has run. */
  public static final int STARTED = 4;

  /** {@link #onResume} has run: the fragment is interactive. */
  public static final int RESUMED = 5;

  static final int F_ADDED = 1;
  static final int F_REMOVING = 2;
  static final int F_DETACHED = 4;
  static final int F_HIDDEN = 8;

  int mState = INITIALIZING;
  int mMaxState = RESUMED;
  int mFlags;

  /** The manager's stable id for this instance, persisted across a host re-creation. */
  int mWho;

  int mContainerId;
  int mBackStackNesting;
  String mTag;
  Bundle mArguments;

  /** State to hand the next {@link #onCreate} and {@link #onCreateView}, from a restore. */
  Bundle mSavedFragmentState;

  View mView;
  ViewGroup mContainer;
  FragmentManager mFragmentManager;
  Activity mHost;

  /** The view's lifecycle, created on the first {@link #getViewLifecycleOwner}; one per view. */
  private Lifecycle mViewLifecycle;

  private final int mContentLayoutId;

  /** A fragment whose view {@link #onCreateView} builds, or none. */
  public Fragment() {
    mContentLayoutId = 0;
  }

  /**
   * A fragment whose default {@link #onCreateView} inflates {@code R.layout.contentLayoutId}.
   * Mirrors Android.
   */
  public Fragment(int contentLayoutId) {
    mContentLayoutId = contentLayoutId;
  }

  // ── Lifecycle callbacks; every default does nothing ──────────────────────

  /** First callback: the fragment now has a host, which {@link #getActivity} returns. */
  public void onAttach(Context context) {}

  /**
   * Create the fragment's non-view state. {@code savedInstanceState} is what {@link
   * #onSaveInstanceState} wrote before the host was re-created, or the Bundle given to {@link
   * #setInitialSavedState}; otherwise {@code null}.
   */
  public void onCreate(Bundle savedInstanceState) {}

  /**
   * Build and return the fragment's view. {@code container} is the parent the view will be added to
   * (inflate with {@code attachToRoot = false}, as on Android), or {@code null} when the fragment
   * was added without a container id. The default inflates the layout given to {@link
   * #Fragment(int)}, or returns {@code null} for a fragment without a UI.
   */
  public View onCreateView(
      LayoutInflater inflater, ViewGroup container, Bundle savedInstanceState) {
    if (mContentLayoutId != 0) {
      return inflater.inflate(mContentLayoutId, container, false);
    }
    return null;
  }

  /** The view from {@link #onCreateView} is in place: find its children here. */
  public void onViewCreated(View view, Bundle savedInstanceState) {}

  /** After {@link #onViewCreated}; nothing more is restored here, the hook exists for parity. */
  public void onViewStateRestored(Bundle savedInstanceState) {}

  public void onStart() {}

  public void onResume() {}

  public void onPause() {}

  public void onStop() {}

  /**
   * Save what the next instance needs; it comes back in {@link #onCreate} and {@link
   * #onCreateView}. Called when the host saves its state and when a {@code ViewPager2} lets go of
   * an off-screen page.
   */
  public void onSaveInstanceState(Bundle outState) {}

  /**
   * The view is about to be freed. Runs while the tree is still live, so animations can be
   * cancelled and listeners cleared; afterwards every view of it is released and {@link #getView}
   * is {@code null}.
   */
  public void onDestroyView() {}

  public void onDestroy() {}

  /** Last callback: the fragment no longer has a host. */
  public void onDetach() {}

  /** Mirrors Android: the fragment was hidden or shown by a transaction. */
  public void onHiddenChanged(boolean hidden) {}

  // ── Accessors ───────────────────────────────────────────────────────────

  /** The host, or {@code null} while not attached. */
  public final Activity getActivity() {
    return mHost;
  }

  public final Activity requireActivity() {
    Activity a = mHost;
    if (a == null) {
      throw new IllegalStateException("Fragment " + this + " not attached to an activity.");
    }
    return a;
  }

  /** The host as a {@link Context}, or {@code null} while not attached. */
  public Context getContext() {
    return mHost;
  }

  public final Context requireContext() {
    return requireActivity();
  }

  /** The manager this fragment was added to. */
  public final FragmentManager getParentFragmentManager() {
    FragmentManager fm = mFragmentManager;
    if (fm == null) {
      throw new IllegalStateException(
          "Fragment " + this + " not associated with a fragment manager.");
    }
    return fm;
  }

  /** The view {@link #onCreateView} returned, or {@code null} before it and after onDestroyView. */
  public View getView() {
    return mView;
  }

  /**
   * The lifecycle of this fragment's view, mirroring {@code Fragment#getViewLifecycleOwner()}: what
   * to give {@link picodroid.lifecycle.LiveData#observe} from {@link #onViewCreated}, so the
   * observer goes away with the view rather than with the fragment. It is created after {@link
   * #onViewStateRestored}, started after {@link #onStart}, resumed after {@link #onResume}, steps
   * back before {@link #onPause} and {@link #onStop}, and is destroyed before {@link
   * #onDestroyView}; each view gets a new one. Available once the view exists, so from {@link
   * #onViewCreated} on (Android also allows it inside {@link #onCreateView}).
   */
  public LifecycleOwner getViewLifecycleOwner() {
    if (mViewLifecycle == null) {
      if (mView == null) {
        throw new IllegalStateException(
            "Can't access the Fragment View's LifecycleOwner when getView() is null i.e., before"
                + " onCreateView() or after onDestroyView()");
      }
      mViewLifecycle = new Lifecycle();
      // INITIALIZED inside onViewCreated: the manager reports the view created after it returns.
      if (mState >= STARTED) {
        mViewLifecycle.setCurrentState(mState >= RESUMED ? Lifecycle.RESUMED : Lifecycle.STARTED);
      } else if (mState == VIEW_CREATED) {
        mViewLifecycle.setCurrentState(Lifecycle.CREATED);
      }
    }
    return mViewLifecycle;
  }

  /** From the manager: the view's lifecycle moved. Costs nothing until someone asked for it. */
  void setViewLifecycleState(int state) {
    Lifecycle lifecycle = mViewLifecycle;
    if (lifecycle != null) {
      lifecycle.setCurrentState(state);
      if (state == Lifecycle.DESTROYED) {
        mViewLifecycle = null;
      }
    }
  }

  public final View requireView() {
    View v = mView;
    if (v == null) {
      throw new IllegalStateException(
          "Fragment "
              + this
              + " did not return a View from onCreateView() or this was called before"
              + " onCreateView().");
    }
    return v;
  }

  public final Bundle getArguments() {
    return mArguments;
  }

  public final Bundle requireArguments() {
    Bundle args = mArguments;
    if (args == null) {
      throw new IllegalStateException("Fragment " + this + " does not have any arguments.");
    }
    return args;
  }

  /**
   * Mirrors Android: construction arguments, kept with the fragment across a host re-creation.
   * Refused once the manager's state has been saved.
   */
  public void setArguments(Bundle args) {
    if (mFragmentManager != null && mFragmentManager.isStateSaved()) {
      throw new IllegalStateException("Fragment already added and state has been saved");
    }
    mArguments = args;
  }

  /**
   * Mirrors Android's {@code setInitialSavedState(SavedState)} with the Bundle {@link
   * FragmentManager#saveFragmentInstanceState} produced: the next {@link #onCreate} and {@link
   * #onCreateView} receive it as their saved state. Only before the fragment is added.
   */
  public void setInitialSavedState(Bundle state) {
    if (mFragmentManager != null) {
      throw new IllegalStateException("Fragment already added");
    }
    mSavedFragmentState = state == null || state.isEmpty() ? null : state;
  }

  public final String getTag() {
    return mTag;
  }

  /** The container id this fragment was added to, or 0. */
  public final int getId() {
    return mContainerId;
  }

  /** Whether the fragment is attached to a host and in its manager's added list. */
  public final boolean isAdded() {
    return mHost != null && (mFlags & F_ADDED) != 0;
  }

  public final boolean isDetached() {
    return (mFlags & F_DETACHED) != 0;
  }

  public final boolean isHidden() {
    return (mFlags & F_HIDDEN) != 0;
  }

  public final boolean isRemoving() {
    return (mFlags & F_REMOVING) != 0;
  }

  public final boolean isResumed() {
    return mState >= RESUMED;
  }

  /** Added, not hidden, and with a visible view. */
  public final boolean isVisible() {
    return isAdded() && !isHidden() && mView != null && mView.getVisibility() == View.VISIBLE;
  }

  public final boolean isStateSaved() {
    return mFragmentManager != null && mFragmentManager.isStateSaved();
  }

  public final LayoutInflater getLayoutInflater() {
    return requireActivity().getLayoutInflater();
  }

  public final Resources getResources() {
    return requireContext().getResources();
  }

  public final String getString(int resId) {
    return requireContext().getString(resId);
  }

  /** Mirrors Android: {@code requireContext().getString(resId, formatArgs)}. */
  public final String getString(int resId, Object... formatArgs) {
    return requireContext().getString(resId, formatArgs);
  }

  public void startActivity(Intent intent) {
    requireActivity().startActivity(intent);
  }

  boolean isInBackStack() {
    return mBackStackNesting > 0;
  }
}
