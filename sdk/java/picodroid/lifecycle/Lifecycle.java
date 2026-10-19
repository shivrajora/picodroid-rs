// SPDX-License-Identifier: GPL-3.0-only
package picodroid.lifecycle;

import java.util.ArrayList;

/**
 * Where a {@link LifecycleOwner} is between creation and destruction, mirroring {@code
 * androidx.lifecycle.Lifecycle} and its {@code LifecycleRegistry} in one class. The framework moves
 * it with {@link #setCurrentState} as the owner's callbacks run: an owner is {@link State#CREATED}
 * after its {@code onCreate}, {@link State#STARTED} after {@code onStart}, {@link State#RESUMED}
 * after {@code onResume}, and steps back down before {@code onPause}, {@code onStop} and {@code
 * onDestroy}, as on Android. Each step tells the observers its {@link Event}: a {@link
 * LifecycleEventObserver} hears every one, a {@link DefaultLifecycleObserver} the matching
 * callback, and a new observer is first brought up to the current state.
 *
 * <p>A Lifecycle is also its own {@link LifecycleOwner}: that is what a fragment hands out for its
 * view, in place of a class per owner. One made with {@link #Lifecycle(LifecycleOwner)} reports
 * that owner to its observers instead.
 */
public class Lifecycle implements LifecycleOwner {
  /** Mirrors {@code androidx.lifecycle.Lifecycle.State}; ordered, so {@link #isAtLeast} works. */
  public enum State {
    /** The owner is gone; a Lifecycle never leaves this state. */
    DESTROYED,

    /** The owner exists and its {@code onCreate} has not returned yet. */
    INITIALIZED,

    CREATED,

    /** From here up the owner is on screen: {@link LiveData} delivers to its observers. */
    STARTED,

    RESUMED;

    /** Mirrors Android: this state is {@code state} or a later one. */
    public boolean isAtLeast(State state) {
      return compareTo(state) >= 0;
    }
  }

  /** Mirrors {@code androidx.lifecycle.Lifecycle.Event}: one step of the owner's lifecycle. */
  public enum Event {
    ON_CREATE,
    ON_START,
    ON_RESUME,
    ON_PAUSE,
    ON_STOP,
    ON_DESTROY,

    /** Never dispatched; the wildcard Android's annotation-driven observers accept. */
    ON_ANY;

    /** Mirrors Android: the state the owner is in once this event has been dispatched. */
    public State getTargetState() {
      if (this == ON_CREATE || this == ON_STOP) {
        return State.CREATED;
      }
      if (this == ON_START || this == ON_PAUSE) {
        return State.STARTED;
      }
      if (this == ON_RESUME) {
        return State.RESUMED;
      }
      if (this == ON_DESTROY) {
        return State.DESTROYED;
      }
      throw new IllegalArgumentException(this + " has no target state");
    }

    /** Mirrors Android: the event that leaves {@code state} upward, or null from the top. */
    public static Event upFrom(State state) {
      if (state == State.INITIALIZED) {
        return ON_CREATE;
      }
      if (state == State.CREATED) {
        return ON_START;
      }
      if (state == State.STARTED) {
        return ON_RESUME;
      }
      return null;
    }

    /** Mirrors Android: the event that leaves {@code state} downward, or null from the bottom. */
    public static Event downFrom(State state) {
      if (state == State.CREATED) {
        return ON_DESTROY;
      }
      if (state == State.STARTED) {
        return ON_STOP;
      }
      if (state == State.RESUMED) {
        return ON_PAUSE;
      }
      return null;
    }

    /** Mirrors Android: the event that arrives at {@code state} from below, or null. */
    public static Event upTo(State state) {
      if (state == State.CREATED) {
        return ON_CREATE;
      }
      if (state == State.STARTED) {
        return ON_START;
      }
      if (state == State.RESUMED) {
        return ON_RESUME;
      }
      return null;
    }

    /** Mirrors Android: the event that arrives at {@code state} from above, or null. */
    public static Event downTo(State state) {
      if (state == State.DESTROYED) {
        return ON_DESTROY;
      }
      if (state == State.CREATED) {
        return ON_STOP;
      }
      if (state == State.STARTED) {
        return ON_PAUSE;
      }
      return null;
    }
  }

  private State mState = State.INITIALIZED;

  /** Who the observers are told about: the owner given at construction, else this Lifecycle. */
  private final LifecycleOwner mOwner;

  /** The observers; null while there are none. */
  private ArrayList<LifecycleObserver> mObservers;

  /** A Lifecycle that is its own owner. */
  public Lifecycle() {
    mOwner = this;
  }

  /** Mirrors {@code LifecycleRegistry(LifecycleOwner)}: the owner the observers are handed. */
  public Lifecycle(LifecycleOwner owner) {
    mOwner = owner == null ? this : owner;
  }

  public State getCurrentState() {
    return mState;
  }

  /**
   * Moves to {@code state} one step at a time, dispatching each step's {@link Event}; mirrors
   * {@code LifecycleRegistry.setCurrentState}. For the framework and for an app's own {@link
   * LifecycleOwner}. Main thread only. A destroyed Lifecycle stays destroyed; one destroyed
   * straight from {@link State#INITIALIZED} passes through {@link State#CREATED} first, as on
   * Android.
   */
  public void setCurrentState(State state) {
    if (state == null || state == mState || mState == State.DESTROYED) {
      return;
    }
    if (mState == State.INITIALIZED && state == State.DESTROYED) {
      step(State.CREATED, Event.ON_CREATE);
    }
    while (mState != state) {
      if (state.isAtLeast(mState)) {
        State next = stepUp(mState);
        step(next, Event.upTo(next));
      } else {
        State next = stepDown(mState);
        step(next, Event.downTo(next));
      }
    }
  }

  /** Mirrors {@code LifecycleRegistry.handleLifecycleEvent}: moves to the event's target state. */
  public void handleLifecycleEvent(Event event) {
    setCurrentState(event.getTargetState());
  }

  /**
   * Mirrors Android: adds {@code observer} and brings it up to the current state, one event at a
   * time ({@code ON_CREATE}, then {@code ON_START}, …). Nothing is replayed on a destroyed
   * Lifecycle, and an observer already added is left as it is.
   */
  public void addObserver(LifecycleObserver observer) {
    if (observer == null) {
      return;
    }
    if (mObservers == null) {
      mObservers = new ArrayList<>();
    } else if (mObservers.contains(observer)) {
      return;
    }
    mObservers.add(observer);
    if (mState == State.DESTROYED) {
      return;
    }
    State at = State.INITIALIZED;
    while (at != mState && mObservers != null && mObservers.contains(observer)) {
      at = stepUp(at);
      tell(observer, Event.upTo(at));
    }
  }

  public void removeObserver(LifecycleObserver observer) {
    if (mObservers != null) {
      mObservers.remove(observer);
    }
  }

  @Override
  public Lifecycle getLifecycle() {
    return this;
  }

  private static State stepUp(State state) {
    if (state == State.INITIALIZED) {
      return State.CREATED;
    }
    if (state == State.CREATED) {
      return State.STARTED;
    }
    return State.RESUMED;
  }

  private static State stepDown(State state) {
    if (state == State.RESUMED) {
      return State.STARTED;
    }
    if (state == State.STARTED) {
      return State.CREATED;
    }
    return State.DESTROYED;
  }

  private void step(State next, Event event) {
    mState = next;
    ArrayList<LifecycleObserver> observers = mObservers;
    if (observers == null) {
      return;
    }
    // Newest first, by index: an observer told ON_DESTROY removes itself, and one that removes
    // another only makes this loop tell someone twice, which LiveData's wrapper ignores.
    for (int i = observers.size() - 1; i >= 0; i--) {
      if (i < observers.size()) {
        tell(observers.get(i), event);
      }
    }
    if (next == State.DESTROYED) {
      mObservers = null;
    }
  }

  private void tell(LifecycleObserver observer, Event event) {
    if (observer instanceof LifecycleEventObserver) {
      ((LifecycleEventObserver) observer).onStateChanged(mOwner, event);
    } else if (observer instanceof DefaultLifecycleObserver) {
      DefaultLifecycleObserver o = (DefaultLifecycleObserver) observer;
      if (event == Event.ON_CREATE) {
        o.onCreate(mOwner);
      } else if (event == Event.ON_START) {
        o.onStart(mOwner);
      } else if (event == Event.ON_RESUME) {
        o.onResume(mOwner);
      } else if (event == Event.ON_PAUSE) {
        o.onPause(mOwner);
      } else if (event == Event.ON_STOP) {
        o.onStop(mOwner);
      } else if (event == Event.ON_DESTROY) {
        o.onDestroy(mOwner);
      }
    }
  }
}
