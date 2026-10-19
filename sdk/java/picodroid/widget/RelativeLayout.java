// SPDX-License-Identifier: GPL-3.0-only
package picodroid.widget;

import picodroid.content.Context;
import picodroid.view.Gravity;
import picodroid.view.View;
import picodroid.view.ViewGroup;

/**
 * A layout whose children are placed by rules, the shape of {@code android.widget.RelativeLayout}:
 * against the parent's edges or centre ({@link #ALIGN_PARENT_RIGHT}, {@link #CENTER_IN_PARENT}), or
 * against a sibling's edges ({@link #BELOW}, {@link #TO_RIGHT_OF}, {@link #ALIGN_LEFT} and the
 * rest). A child constrained on both edges of an axis is stretched between them; one with no rule
 * on an axis sits at the start of it, by its margins. The rules are solved in dependency order,
 * whichever order the children were added in; a circular dependency is broken by placing the
 * children in the cycle as if unanchored (Android throws).
 *
 * <p>A sibling is named by its id, as on Android, so a rule may name a child added later. A child's
 * id is read when it is added ({@link #addView}): give the child its id first.
 *
 * <p>Divergences: {@link #ALIGN_BASELINE} is accepted and ignored; {@code START}/{@code END} rules
 * are {@code LEFT}/{@code RIGHT} (left-to-right only); a GONE sibling is a missing one (Android
 * follows its rules through); at most 20 children are placed by rules.
 */
public class RelativeLayout extends ViewGroup {
  /** The value of a boolean rule that is set. */
  public static final int TRUE = -1;

  public static final int LEFT_OF = 0;
  public static final int RIGHT_OF = 1;
  public static final int ABOVE = 2;
  public static final int BELOW = 3;
  public static final int ALIGN_BASELINE = 4;
  public static final int ALIGN_LEFT = 5;
  public static final int ALIGN_TOP = 6;
  public static final int ALIGN_RIGHT = 7;
  public static final int ALIGN_BOTTOM = 8;
  public static final int ALIGN_PARENT_LEFT = 9;
  public static final int ALIGN_PARENT_TOP = 10;
  public static final int ALIGN_PARENT_RIGHT = 11;
  public static final int ALIGN_PARENT_BOTTOM = 12;
  public static final int CENTER_IN_PARENT = 13;
  public static final int CENTER_HORIZONTAL = 14;
  public static final int CENTER_VERTICAL = 15;
  public static final int START_OF = 16;
  public static final int END_OF = 17;
  public static final int ALIGN_START = 18;
  public static final int ALIGN_END = 19;
  public static final int ALIGN_PARENT_START = 20;
  public static final int ALIGN_PARENT_END = 21;

  private static final int VERB_COUNT = 22;

  // The rule flags as the runtime reads them (graphics/lvgl/layouts/relative.rs F_*).
  static final int F_PARENT_LEFT = 1;
  static final int F_PARENT_TOP = 1 << 1;
  static final int F_PARENT_RIGHT = 1 << 2;
  static final int F_PARENT_BOTTOM = 1 << 3;
  static final int F_CENTER_IN_PARENT = 1 << 4;
  static final int F_CENTER_HORIZONTAL = 1 << 5;
  static final int F_CENTER_VERTICAL = 1 << 6;
  static final int F_IF_MISSING = 1 << 7;

  // The anchor slots as the runtime reads them (relative.rs A_*).
  static final int A_ABOVE = 0;
  static final int A_BELOW = 1;
  static final int A_TO_LEFT_OF = 2;
  static final int A_TO_RIGHT_OF = 3;
  static final int A_ALIGN_LEFT = 4;
  static final int A_ALIGN_TOP = 5;
  static final int A_ALIGN_RIGHT = 6;
  static final int A_ALIGN_BOTTOM = 7;
  static final int ANCHORS = 8;

  private int mGravity = Gravity.START | Gravity.TOP;

  public RelativeLayout() {
    super(nativeCreate());
  }

  public RelativeLayout(Context ctx) {
    super(nativeCreate());
  }

  /** For the inflater's native path, which makes the widget itself ({@link InflaterSupport}). */
  protected RelativeLayout(int nativeHandle) {
    super(nativeHandle);
  }

  private static native int nativeCreate();

  /**
   * Where the children go as a group when their rules leave room: the group's bounding box is
   * placed where the {@link Gravity} bits say. Mirrors {@code RelativeLayout.setGravity}.
   */
  public void setGravity(int gravity) {
    mGravity = gravity;
    nativeSetGravity(gravity);
  }

  public int getGravity() {
    return mGravity;
  }

  public void setHorizontalGravity(int horizontalGravity) {
    setGravity(
        (mGravity & ~Gravity.RELATIVE_HORIZONTAL_GRAVITY_MASK)
            | (horizontalGravity & Gravity.RELATIVE_HORIZONTAL_GRAVITY_MASK));
  }

  public void setVerticalGravity(int verticalGravity) {
    setGravity(
        (mGravity & ~Gravity.VERTICAL_GRAVITY_MASK)
            | (verticalGravity & Gravity.VERTICAL_GRAVITY_MASK));
  }

  private native void nativeSetGravity(int gravity);

  @Override
  public void addView(View child) {
    ViewGroup.LayoutParams params = child == null ? null : child.getLayoutParams();
    addView(
        child,
        params != null
            ? params
            : new LayoutParams(
                ViewGroup.LayoutParams.WRAP_CONTENT, ViewGroup.LayoutParams.WRAP_CONTENT));
  }

  /**
   * Adds {@code child} with its rules. Params of another type place the child as if unruled (at the
   * start of both axes, by its margins); the child's id is recorded either way, so a sibling's rule
   * can name it.
   */
  @Override
  public void addView(View child, ViewGroup.LayoutParams params) {
    super.addView(child, params);
    if (params instanceof LayoutParams) {
      LayoutParams lp = (LayoutParams) params;
      setRelativeRules(child, child.getId(), lp.flags(), lp.mAnchors);
    } else {
      setRelativeRules(child, child.getId(), 0, null);
    }
  }

  /**
   * Mirrors {@code android.widget.RelativeLayout.LayoutParams}: the rules, by verb, and the
   * margins. Rules are read when the child is added: set them before {@link #addView}.
   */
  public static class LayoutParams extends ViewGroup.MarginLayoutParams {
    /** When a sibling rule names a view that is not here, apply the parent-edge rule instead. */
    public boolean alignWithParent;

    int mFlags;

    /** The id each sibling verb names, by runtime slot; null until a sibling rule is added. */
    int[] mAnchors;

    public LayoutParams(int width, int height) {
      super(width, height);
    }

    public LayoutParams(ViewGroup.LayoutParams source) {
      super(source);
    }

    public LayoutParams(ViewGroup.MarginLayoutParams source) {
      super(source);
    }

    public LayoutParams(LayoutParams source) {
      super(source);
      alignWithParent = source.alignWithParent;
      mFlags = source.mFlags;
      mAnchors = source.mAnchors == null ? null : source.mAnchors.clone();
    }

    /** Sets a boolean rule ({@link #ALIGN_PARENT_LEFT}, {@link #CENTER_IN_PARENT}, ...). */
    public void addRule(int verb) {
      addRule(verb, TRUE);
    }

    /**
     * Sets a rule: {@code subject} is {@link #TRUE} for a boolean verb, a view id for a sibling
     * verb ({@link #BELOW}, {@link #TO_RIGHT_OF}, ...), or 0 to remove it.
     */
    public void addRule(int verb, int subject) {
      int flag = flagOf(verb);
      if (flag != 0) {
        if (subject != 0) {
          mFlags |= flag;
        } else {
          mFlags &= ~flag;
        }
        return;
      }
      int slot = slotOf(verb);
      if (slot < 0) {
        if (verb < 0 || verb >= VERB_COUNT) {
          throw new IllegalArgumentException("RelativeLayout: unknown rule " + verb);
        }
        return; // ALIGN_BASELINE: accepted, not applied
      }
      if (mAnchors == null) {
        if (subject == 0) {
          return;
        }
        mAnchors = new int[ANCHORS];
      }
      mAnchors[slot] = subject == TRUE ? 0 : subject;
    }

    public void removeRule(int verb) {
      addRule(verb, 0);
    }

    /** The rule's subject: {@link #TRUE}, a view id, or 0 when the rule is not set. */
    public int getRule(int verb) {
      int flag = flagOf(verb);
      if (flag != 0) {
        return (mFlags & flag) != 0 ? TRUE : 0;
      }
      int slot = slotOf(verb);
      return slot < 0 || mAnchors == null ? 0 : mAnchors[slot];
    }

    /** Every rule by verb, as Android's does ({@code int[22]}): built on each call. */
    public int[] getRules() {
      int[] rules = new int[VERB_COUNT];
      for (int verb = 0; verb < VERB_COUNT; verb++) {
        rules[verb] = getRule(verb);
      }
      return rules;
    }

    /** The flag of a boolean verb, 0 for a sibling verb. */
    private static int flagOf(int verb) {
      switch (verb) {
        case ALIGN_PARENT_LEFT:
        case ALIGN_PARENT_START:
          return F_PARENT_LEFT;
        case ALIGN_PARENT_TOP:
          return F_PARENT_TOP;
        case ALIGN_PARENT_RIGHT:
        case ALIGN_PARENT_END:
          return F_PARENT_RIGHT;
        case ALIGN_PARENT_BOTTOM:
          return F_PARENT_BOTTOM;
        case CENTER_IN_PARENT:
          return F_CENTER_IN_PARENT;
        case CENTER_HORIZONTAL:
          return F_CENTER_HORIZONTAL;
        case CENTER_VERTICAL:
          return F_CENTER_VERTICAL;
        default:
          return 0;
      }
    }

    /** The anchor slot of a sibling verb, -1 for a boolean one or {@link #ALIGN_BASELINE}. */
    private static int slotOf(int verb) {
      switch (verb) {
        case ABOVE:
          return A_ABOVE;
        case BELOW:
          return A_BELOW;
        case LEFT_OF:
        case START_OF:
          return A_TO_LEFT_OF;
        case RIGHT_OF:
        case END_OF:
          return A_TO_RIGHT_OF;
        case ALIGN_LEFT:
        case ALIGN_START:
          return A_ALIGN_LEFT;
        case ALIGN_TOP:
          return A_ALIGN_TOP;
        case ALIGN_RIGHT:
        case ALIGN_END:
          return A_ALIGN_RIGHT;
        case ALIGN_BOTTOM:
          return A_ALIGN_BOTTOM;
        default:
          return -1;
      }
    }

    /** The verb flags as the runtime reads them, with {@link #alignWithParent} folded in. */
    int flags() {
      return alignWithParent ? mFlags | F_IF_MISSING : mFlags;
    }
  }
}
