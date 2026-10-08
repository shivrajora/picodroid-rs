// SPDX-License-Identifier: GPL-3.0-only
package picodroid.view;

/**
 * A hardware key press, repeat or release. Mirrors {@code android.view.KeyEvent}: one instance per
 * edge, delivered first to the focused view's {@link OnKeyListener} and then, if unconsumed, to the
 * foreground Activity through {@link #dispatch}.
 *
 * <p>A key held past {@link ViewConfiguration#getKeyRepeatTimeout} produces further {@link
 * #ACTION_DOWN} events with a rising {@link #getRepeatCount} every {@link
 * ViewConfiguration#getKeyRepeatDelay}, as Android's input dispatcher does. The first repeat
 * carries {@link #FLAG_LONG_PRESS}: that is the event {@link Callback#onKeyLongPress} is offered
 * when the press was {@linkplain #startTracking tracked}. The release of a press whose long-press
 * was handled carries {@link #FLAG_CANCELED}, so a handler that acts on the release ({@code
 * isTracking() && !isCanceled()}) does not also run the short-press action. That is how Android
 * gives one button two actions.
 *
 * <p>The framework recycles one instance for every key edge, so an event must not be kept past the
 * callback it arrives in.
 */
public class KeyEvent {
  public static final int ACTION_DOWN = 0;
  public static final int ACTION_UP = 1;

  public static final int KEYCODE_HOME = 3;
  public static final int KEYCODE_BACK = 4;
  public static final int KEYCODE_DPAD_UP = 19;
  public static final int KEYCODE_DPAD_DOWN = 20;
  public static final int KEYCODE_DPAD_LEFT = 21;
  public static final int KEYCODE_DPAD_RIGHT = 22;
  public static final int KEYCODE_DPAD_CENTER = 23;

  // Android's codes for a gamepad's face buttons. No board maps a button to them yet (a board's
  // buttons arrive as the DPAD / BACK codes its board.toml names), so today these are for an app
  // that injects or compares them itself.
  public static final int KEYCODE_BUTTON_A = 96;
  public static final int KEYCODE_BUTTON_B = 97;
  public static final int KEYCODE_BUTTON_X = 99;
  public static final int KEYCODE_BUTTON_Y = 100;

  /**
   * Set on the release of a press whose long-press was handled, or whose dispatch was otherwise
   * abandoned: the release must not perform its action. Android's value.
   */
  public static final int FLAG_CANCELED = 0x20;

  /**
   * Set on the first repeat after the long-press timeout — the event {@link
   * Callback#onKeyLongPress} sees. Android's value.
   */
  public static final int FLAG_LONG_PRESS = 0x80;

  /** With {@link #FLAG_CANCELED}: the cancel is because a long-press ran. Android's value. */
  public static final int FLAG_CANCELED_LONG_PRESS = 0x100;

  /**
   * Set on a release whose press was tracked with {@link #startTracking}; read through {@link
   * #isTracking}. Android's value.
   */
  public static final int FLAG_TRACKING = 0x200;

  /** Set by {@link #startTracking} on a press; consumed by {@link #dispatch}. Android's value. */
  public static final int FLAG_START_TRACKING = 0x40000000;

  // Field order is the native slot order (graphics/fields.rs::key_event); append only. The
  // dispatcher rewrites every field per event, `flags` included, so nothing carries between edges
  // except through a DispatcherState.
  private int action;
  private int keyCode;
  private int repeatCount;
  private int flags;

  /** {@code SystemClock.elapsedRealtime()} of the press this event belongs to. */
  private long downTime;

  /** {@code SystemClock.elapsedRealtime()} of this edge (for a repeat, when it was synthesised). */
  private long eventTime;

  KeyEvent(int action, int keyCode) {
    this.action = action;
    this.keyCode = keyCode;
  }

  public int getAction() {
    return action;
  }

  public int getKeyCode() {
    return keyCode;
  }

  /**
   * 0 for the press itself, then 1, 2, 3… for each auto-repeat while the key stays held. A handler
   * that acts on {@link #ACTION_DOWN} and wants one action per physical press tests for 0; one that
   * wants to accelerate while held uses the count as a pace.
   */
  public int getRepeatCount() {
    return repeatCount;
  }

  public int getFlags() {
    return flags;
  }

  /** When the key went down, in {@code SystemClock.elapsedRealtime()} milliseconds. */
  public long getDownTime() {
    return downTime;
  }

  /** When this edge happened, in {@code SystemClock.elapsedRealtime()} milliseconds. */
  public long getEventTime() {
    return eventTime;
  }

  /** Whether this is the first repeat after the long-press timeout ({@link #FLAG_LONG_PRESS}). */
  public final boolean isLongPress() {
    return (flags & FLAG_LONG_PRESS) != 0;
  }

  /**
   * Whether this release must not perform its action ({@link #FLAG_CANCELED}), because the press's
   * long-press was handled. Test it beside {@link #isTracking} in {@code onKeyUp}.
   */
  public final boolean isCanceled() {
    return (flags & FLAG_CANCELED) != 0;
  }

  /**
   * Call from {@code onKeyDown} on the press ({@code getRepeatCount() == 0}) to be offered its
   * long-press and to recognise its release: {@link #dispatch} then records the key, calls {@link
   * Callback#onKeyLongPress} on the long-press repeat, and marks the release {@link #isTracking}.
   * The default {@code Activity.onKeyDown} does this for BACK so that {@code onKeyUp} only runs
   * {@code onBackPressed} when the press was not taken by an override. Mirrors Android's {@code
   * KeyEvent.startTracking()}.
   */
  public final void startTracking() {
    flags |= FLAG_START_TRACKING;
  }

  /**
   * On a release: whether {@link #startTracking} was called on the press it belongs to and no other
   * key's press intervened.
   */
  public final boolean isTracking() {
    return (flags & FLAG_TRACKING) != 0;
  }

  /**
   * Deliver this event to {@code receiver}, keeping the press-to-release bookkeeping in {@code
   * state}. Mirrors {@code android.view.KeyEvent#dispatch(Callback, DispatcherState, Object)}: a
   * consumed press that called {@link #startTracking} is recorded; the long-press repeat of a
   * recorded key is offered to {@link Callback#onKeyLongPress}, and if that consumes it the release
   * is {@linkplain #isCanceled cancelled}; a release of the recorded key is marked {@link
   * #isTracking}. The Activity calls this for every event no view took.
   *
   * @param target the object the press is recorded against ({@code DispatcherState.reset(Object)}
   *     forgets one target's press); the receiver, typically
   */
  public final boolean dispatch(Callback receiver, DispatcherState state, Object target) {
    switch (action) {
      case ACTION_DOWN:
        {
          flags &= ~FLAG_START_TRACKING;
          boolean res = receiver.onKeyDown(keyCode, this);
          if (state != null) {
            if (res && repeatCount == 0 && (flags & FLAG_START_TRACKING) != 0) {
              state.startTracking(this, target);
            } else if (isLongPress() && state.isTracking(this)) {
              if (receiver.onKeyLongPress(keyCode, this)) {
                state.performedLongPress(this);
                res = true;
              }
            }
          }
          return res;
        }
      case ACTION_UP:
        if (state != null) {
          state.handleUpEvent(this);
        }
        return receiver.onKeyUp(keyCode, this);
      default:
        return false;
    }
  }

  /**
   * The four key callbacks, as {@code android.view.KeyEvent.Callback}. {@code Activity} implements
   * it; return {@code true} from any of them to consume the event.
   */
  public interface Callback {
    /** A press ({@code getRepeatCount() == 0}) or an auto-repeat while the key stays held. */
    boolean onKeyDown(int keyCode, KeyEvent event);

    /**
     * The key has been held past the long-press timeout, and its press was {@linkplain
     * KeyEvent#startTracking tracked}. Consuming it cancels the release's action.
     */
    boolean onKeyLongPress(int keyCode, KeyEvent event);

    /** The key was released. */
    boolean onKeyUp(int keyCode, KeyEvent event);

    /** Android's batched-repeat callback; this framework never sends one. */
    boolean onKeyMultiple(int keyCode, int count, KeyEvent event);
  }

  /**
   * The press-to-release bookkeeping {@link #dispatch} keeps for one receiver: which key's press
   * was tracked and against what, and which keys had their long-press handled. Mirrors {@code
   * android.view.KeyEvent.DispatcherState}; the Activity owns one.
   */
  public static class DispatcherState {
    /** How many keys can have a handled long-press outstanding at once: one per finger. */
    private static final int MAX_LONG_PRESSES = 4;

    private int downKeyCode;
    private Object downTarget;

    /** Keycodes whose long-press was handled and whose release is still to come; 0 is empty. */
    private final int[] activeLongPresses = new int[MAX_LONG_PRESSES];

    /** Forget every press and long-press: nothing tracked, nothing to cancel. */
    public void reset() {
      downKeyCode = 0;
      downTarget = null;
      for (int i = 0; i < MAX_LONG_PRESSES; i++) {
        activeLongPresses[i] = 0;
      }
    }

    /** Forget the tracked press if it was recorded against {@code target}. */
    public void reset(Object target) {
      if (downTarget == target) {
        downKeyCode = 0;
        downTarget = null;
      }
    }

    /** Record a press; only a press can be tracked. */
    public void startTracking(KeyEvent event, Object target) {
      if (event.getAction() != ACTION_DOWN) {
        throw new IllegalArgumentException("Can only start tracking on a down event");
      }
      downKeyCode = event.getKeyCode();
      downTarget = target;
    }

    /** Whether {@code event}'s key is the one whose press was recorded. */
    public boolean isTracking(KeyEvent event) {
      return downKeyCode == event.getKeyCode();
    }

    /** Note that {@code event}'s key had its long-press handled: its release is cancelled. */
    public void performedLongPress(KeyEvent event) {
      int code = event.getKeyCode();
      int free = -1;
      for (int i = 0; i < MAX_LONG_PRESSES; i++) {
        if (activeLongPresses[i] == code) {
          return;
        }
        if (free < 0 && activeLongPresses[i] == 0) {
          free = i;
        }
      }
      if (free >= 0) {
        activeLongPresses[free] = code;
      }
    }

    /**
     * Flag a release: {@link #FLAG_CANCELED} (with {@link #FLAG_CANCELED_LONG_PRESS}) when the
     * press's long-press was handled, {@link #FLAG_TRACKING} when its press was recorded.
     */
    public void handleUpEvent(KeyEvent event) {
      int code = event.getKeyCode();
      for (int i = 0; i < MAX_LONG_PRESSES; i++) {
        if (activeLongPresses[i] == code) {
          event.flags |= FLAG_CANCELED | FLAG_CANCELED_LONG_PRESS;
          activeLongPresses[i] = 0;
          break;
        }
      }
      if (downKeyCode == code) {
        event.flags |= FLAG_TRACKING;
        downKeyCode = 0;
        downTarget = null;
      }
    }
  }
}
