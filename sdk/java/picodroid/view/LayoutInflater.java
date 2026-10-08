// SPDX-License-Identifier: GPL-3.0-only
package picodroid.view;

import picodroid.content.Context;
import picodroid.content.res.ColorStateList;
import picodroid.content.res.Resources;
import picodroid.graphics.drawable.GradientDrawable;
import picodroid.os.SystemClock;
import picodroid.text.TextUtils;
import picodroid.util.AttributeSet;
import picodroid.util.TypedValue;
import picodroid.widget.Button;
import picodroid.widget.CheckBox;
import picodroid.widget.CircularProgressIndicator;
import picodroid.widget.CompoundButton;
import picodroid.widget.EditText;
import picodroid.widget.FrameLayout;
import picodroid.widget.ImageView;
import picodroid.widget.LinearLayout;
import picodroid.widget.ListView;
import picodroid.widget.ProgressBar;
import picodroid.widget.RadioButton;
import picodroid.widget.RadioGroup;
import picodroid.widget.ScrollView;
import picodroid.widget.SeekBar;
import picodroid.widget.Spinner;
import picodroid.widget.Switch;
import picodroid.widget.TextView;
import picodroid.widget.ToggleButton;

/**
 * Builds a view tree from a compiled {@code res/layout/*.xml}. Mirrors {@code
 * android.view.LayoutInflater}.
 *
 * <p>Gradle compiles each layout into a stream of 32-bit words inside the app's PAPK (the format is
 * documented in {@code crates/papk-format/src/res.rs}); no XML reaches the device. Every reference
 * in the XML — {@code @color/}, {@code @dimen/}, {@code 12dp} — is already a number by then; only
 * strings are looked up at inflation, through {@link Resources}.
 *
 * <p>The elements that can be inflated are the framework widgets named in {@code create}, and any
 * view class of the app's own that a {@link Factory} makes: there is no reflection to construct one
 * by name, so the layout names the class and the factory (by default the Activity, through {@code
 * Activity.onCreateView(String, Context, AttributeSet)}) says how. Attributes the framework has no
 * setter for ({@code textStyle}, …) are reported and dropped at build time.
 */
public class LayoutInflater {
  // The element (CLASS_*) and attribute (ATTR_*) codes of the layout word stream appear below as
  // literal case labels, each named in a trailing comment. They are not static finals on purpose:
  // some sixty constant fields would cost this class ~2 KB of flash on every board.
  // papk_format::res::layout holds the same numbers for the compiler, and a papk-pack test
  // (codes_match_the_java_sdk) reads these comments and fails when the two drift. Append only.

  /**
   * Makes the views the framework cannot: the classes of the app's own that a layout names. Mirrors
   * {@code android.view.LayoutInflater.Factory}.
   */
  public interface Factory {
    /**
     * A new view for the layout element {@code name} (a fully qualified class name), or {@code
     * null} to decline. The framework applies the element's attributes to what is returned; {@code
     * attrs} is always empty.
     */
    View onCreateView(String name, Context context, AttributeSet attrs);
  }

  /** What a {@link Factory} is handed: a layout's attributes are applied by the inflater. */
  private static final AttributeSet NO_ATTRIBUTES = () -> 0;

  private final Context mContext;

  private Factory mFactory;

  /** The layout being inflated and the read position in its word stream. */
  private int mLayout;

  private int mPos;

  /** The child count of the node {@link #one} last made. */
  private int mChildCount;

  // An inflation in slices (begin / resume / finish), for AsyncLayoutInflater: the groups still
  // waiting for children, innermost last, and how many each is waiting for.
  private ViewGroup[] mOpen;
  private int[] mOpenLeft;
  private int mDepth;
  private ViewGroup mRootParent;
  private View mRoot;
  private int mRootVisibility;

  private LayoutInflater(Context context) {
    mContext = context;
    if (context instanceof Factory) {
      mFactory = (Factory) context; // an Activity makes its own custom views, as on Android
    }
  }

  /** Mirrors Android: the inflater for {@code context}. */
  public static LayoutInflater from(Context context) {
    return new LayoutInflater(context);
  }

  /**
   * Mirrors Android: the factory asked for the views a layout names by class. Unlike Android's, it
   * may be replaced.
   */
  public void setFactory(Factory factory) {
    if (factory == null) {
      throw new NullPointerException("Given factory can not be null");
    }
    mFactory = factory;
  }

  /** Mirrors Android: the factory in use, or {@code null}. */
  public Factory getFactory() {
    return mFactory;
  }

  /** Mirrors Android: the context this inflater creates views with. */
  public Context getContext() {
    return mContext;
  }

  /** Mirrors Android: {@code inflate(resource, root, root != null)}. */
  public View inflate(int resource, ViewGroup root) {
    return inflate(resource, root, root != null);
  }

  /**
   * Mirrors Android: inflates {@code R.layout.*}. With a {@code root}, the layout's top-level
   * {@code layout_*} attributes become the {@code LayoutParams} {@code root} wants; with {@code
   * attachToRoot} the new tree is also added to {@code root}, and {@code root} is what is returned.
   *
   * @throws Resources.NotFoundException if {@code resource} is not a layout of this app
   */
  public View inflate(int resource, ViewGroup root, boolean attachToRoot) {
    mLayout = resource;
    mPos = 0;
    View view = node(root);
    if (root != null && attachToRoot) {
      root.addView(view, view.getLayoutParams());
      return root;
    }
    if (root == null) {
      sizeUnparented(view);
    }
    return view;
  }

  /**
   * Nothing will size this root for us. Android drops the root's layout_* here; keeping them is
   * what lets setContentView(R.layout.x) honour an explicit size.
   */
  private static void sizeUnparented(View view) {
    ViewGroup.LayoutParams lp = view.getLayoutParams();
    if (lp.width != ViewGroup.LayoutParams.WRAP_CONTENT
        || lp.height != ViewGroup.LayoutParams.WRAP_CONTENT) {
      view.setSize(lp.width, lp.height);
    }
  }

  // ── In slices (AsyncLayoutInflater) ────────────────────────────────────────

  /** Starts inflating {@code resource} for {@link #resume} to carry on. */
  void begin(int resource, ViewGroup root) {
    mLayout = resource;
    mPos = 0;
    mDepth = 0;
    mRoot = null;
    mRootParent = root;
    if (mOpen == null) {
      mOpen = new ViewGroup[8];
      mOpenLeft = new int[8];
    }
  }

  /**
   * Builds views until the tree is whole or {@code SystemClock.elapsedRealtime()} reaches {@code
   * deadlineMs}, always at least one. Returns whether there is more to build.
   */
  boolean resume(long deadlineMs) {
    do {
      View v = one(mDepth == 0 ? mRootParent : mOpen[mDepth - 1]);
      int children = mChildCount;
      if (mDepth == 0) {
        mRoot = v;
        // The tree is built where every new view is, on the screen, over several frames: keep
        // it out of sight until it is whole and its owner has put it where it belongs.
        mRootVisibility = v.getVisibility();
        v.setVisibility(View.INVISIBLE);
      } else {
        mOpen[mDepth - 1].addView(v, v.getLayoutParams());
        mOpenLeft[mDepth - 1]--;
      }
      if (children > 0) {
        if (mDepth == mOpen.length) {
          ViewGroup[] open = new ViewGroup[mDepth * 2];
          int[] left = new int[mDepth * 2];
          System.arraycopy(mOpen, 0, open, 0, mDepth);
          System.arraycopy(mOpenLeft, 0, left, 0, mDepth);
          mOpen = open;
          mOpenLeft = left;
        }
        mOpen[mDepth] = (ViewGroup) v; // the compiler only nests under ViewGroup classes
        mOpenLeft[mDepth] = children;
        mDepth++;
      }
      while (mDepth > 0 && mOpenLeft[mDepth - 1] == 0) {
        mOpen[--mDepth] = null;
      }
      if (mDepth == 0) {
        return false;
      }
    } while (SystemClock.elapsedRealtime() < deadlineMs);
    return true;
  }

  /** The tree {@link #resume} finished, visible again as its layout says. */
  View finish() {
    View view = mRoot;
    ViewGroup parent = mRootParent;
    mRoot = null;
    mRootParent = null;
    view.setVisibility(mRootVisibility);
    if (parent == null) {
      sizeUnparented(view);
    }
    return view;
  }

  /** Frees whatever {@link #resume} has built so far; {@link #begin} may start it again. */
  void abandon() {
    View partial = mRoot;
    mRoot = null;
    while (mDepth > 0) {
      mOpen[--mDepth] = null;
    }
    if (partial != null) {
      partial.close();
    }
  }

  private static native int nativeWord(int layout, int index);

  private int next() {
    return nativeWord(mLayout, mPos++);
  }

  /** Inflates the node at the read position and everything under it. */
  private View node(ViewGroup parent) {
    View v = one(parent);
    int children = mChildCount;
    if (children > 0) {
      // The compiler only nests under the ViewGroup classes create() knows.
      ViewGroup group = (ViewGroup) v;
      for (int i = 0; i < children; i++) {
        View child = node(group);
        group.addView(child, child.getLayoutParams());
      }
    }
    return v;
  }

  /**
   * Makes the one view at the read position, its LayoutParams made for {@code parent}, and leaves
   * the number of children that follow it in {@link #mChildCount}.
   */
  private View one(ViewGroup parent) {
    int header = next();
    int attrs = (header >> 8) & 0xFF;
    mChildCount = header >>> 16;
    int cls = header & 0xFF;
    View v;
    if (cls == 19) {
      // CLASS_CUSTOM, a view class of the app's own: the node's first attribute is
      // ATTR_CLASS_NAME, and a Factory makes the view.
      next();
      v = custom(string(next()));
      attrs--;
    } else {
      v = create(cls);
    }

    int width = ViewGroup.LayoutParams.WRAP_CONTENT;
    int height = ViewGroup.LayoutParams.WRAP_CONTENT;
    float weight = 0;
    int layoutGravity = Gravity.NO_GRAVITY;
    int marginLeft = 0;
    int marginTop = 0;
    int marginRight = 0;
    int marginBottom = 0;
    boolean padded = false;
    int padLeft = 0;
    int padTop = 0;
    int padRight = 0;
    int padBottom = 0;
    boolean filled = false;
    int fill = 0;
    int radius = 0;
    int strokeWidth = 0;
    int strokeColor = 0;

    for (int i = 0; i < attrs; i++) {
      int attr = next();
      int value = next();
      switch (attr) {
        case 2: // ATTR_LAYOUT_WIDTH
          width = value;
          break;
        case 3: // ATTR_LAYOUT_HEIGHT
          height = value;
          break;
        case 4: // ATTR_LAYOUT_WEIGHT
          weight = Float.intBitsToFloat(value);
          break;
        case 5: // ATTR_LAYOUT_GRAVITY
          layoutGravity = value;
          break;
        case 6: // ATTR_PADDING_LEFT
          padLeft = value;
          padded = true;
          break;
        case 7: // ATTR_PADDING_TOP
          padTop = value;
          padded = true;
          break;
        case 8: // ATTR_PADDING_RIGHT
          padRight = value;
          padded = true;
          break;
        case 9: // ATTR_PADDING_BOTTOM
          padBottom = value;
          padded = true;
          break;
        case 10: // ATTR_BACKGROUND
          fill = value;
          filled = true;
          break;
        case 43: // ATTR_LAYOUT_MARGIN_LEFT
          marginLeft = value;
          break;
        case 44: // ATTR_LAYOUT_MARGIN_TOP
          marginTop = value;
          break;
        case 45: // ATTR_LAYOUT_MARGIN_RIGHT
          marginRight = value;
          break;
        case 46: // ATTR_LAYOUT_MARGIN_BOTTOM
          marginBottom = value;
          break;
        case 48: // ATTR_BACKGROUND_RADIUS
          radius = value;
          break;
        case 49: // ATTR_BACKGROUND_STROKE_WIDTH
          strokeWidth = value;
          break;
        case 50: // ATTR_BACKGROUND_STROKE_COLOR
          strokeColor = value;
          break;
        default:
          apply(v, attr, value);
          break;
      }
    }
    if (padded) {
      v.setPadding(padLeft, padTop, padRight, padBottom);
    }
    if (radius != 0 || strokeWidth != 0) {
      // A <shape> drawable: the compiler flattened it into these words.
      GradientDrawable shape = new GradientDrawable().setColor(fill).setCornerRadius(radius);
      if (strokeWidth != 0) {
        shape.setStroke(strokeWidth, strokeColor);
      }
      v.setBackground(shape);
    } else if (filled) {
      v.setBackgroundColor(fill);
    }

    // A bar has no content to wrap: LVGL would size it to zero. wrap_content keeps the size the
    // widget was created with instead — its intrinsic size, as Android's ProgressBar has one.
    if (v instanceof ProgressBar || v instanceof SeekBar) {
      if (width == ViewGroup.LayoutParams.WRAP_CONTENT) {
        width = v.getWidth();
      }
      if (height == ViewGroup.LayoutParams.WRAP_CONTENT) {
        height = v.getHeight();
      }
    }

    ViewGroup.MarginLayoutParams lp;
    if (parent instanceof LinearLayout) {
      LinearLayout.LayoutParams llp = new LinearLayout.LayoutParams(width, height, weight);
      llp.gravity = layoutGravity;
      lp = llp;
    } else if (parent instanceof FrameLayout) {
      lp = new FrameLayout.LayoutParams(width, height, layoutGravity);
    } else {
      lp = new ViewGroup.MarginLayoutParams(width, height);
    }
    if ((marginLeft | marginTop | marginRight | marginBottom) != 0) {
      lp.setMargins(marginLeft, marginTop, marginRight, marginBottom);
    }
    v.setLayoutParams(lp);
    return v;
  }

  /** A view of the app's own, by class name: only a {@link Factory} can make one. */
  private View custom(String name) {
    View v = mFactory == null ? null : mFactory.onCreateView(name, mContext, NO_ATTRIBUTES);
    if (v == null) {
      throw new InflateException(
          "Error inflating class "
              + name
              + ": no LayoutInflater.Factory made it (there is no reflection; override"
              + " Activity.onCreateView(String, Context, AttributeSet))");
    }
    return v;
  }

  private View create(int cls) {
    switch (cls) {
      case 1: // CLASS_LINEAR_LAYOUT
        return new LinearLayout(mContext);
      case 2: // CLASS_FRAME_LAYOUT
        return new FrameLayout(mContext);
      case 3: // CLASS_SCROLL_VIEW
        return new ScrollView(mContext);
      case 4: // CLASS_TEXT_VIEW
        return new TextView(mContext);
      case 5: // CLASS_BUTTON
        return new Button(mContext);
      case 6: // CLASS_IMAGE_VIEW
        return new ImageView(mContext);
      case 7: // CLASS_EDIT_TEXT
        return new EditText(mContext);
      case 8: // CLASS_CHECK_BOX
        return new CheckBox(mContext);
      case 9: // CLASS_SWITCH
        return new Switch(mContext);
      case 10: // CLASS_PROGRESS_BAR
        return new ProgressBar(mContext);
      case 11: // CLASS_SEEK_BAR
        return new SeekBar(mContext);
      case 12: // CLASS_RADIO_GROUP
        return new RadioGroup(mContext);
      case 13: // CLASS_RADIO_BUTTON
        return new RadioButton(mContext);
      case 14: // CLASS_TOGGLE_BUTTON
        return new ToggleButton(mContext);
      case 15: // CLASS_SPINNER
        return new Spinner(mContext);
      case 16: // CLASS_LIST_VIEW
        return new ListView(mContext);
      case 17: // CLASS_CIRCULAR_PROGRESS_INDICATOR
        return new CircularProgressIndicator(mContext);
      case 18: // CLASS_VIEW_PAGER2
        return new picodroid.widget.ViewPager2(mContext);
      case 19: // CLASS_CUSTOM
        // one() makes these: it has the class name to read first.
        throw new InflateException("a custom view without its class name");
      case 20: // CLASS_VIEW
        return new View(mContext);
      default:
        // A layout compiled for a newer framework than this one.
        throw new InflateException("unknown view class code " + cls);
    }
  }

  /** Applies one non-layout attribute. One the view has no setter for is skipped, as on Android. */
  private void apply(View v, int attr, int value) {
    switch (attr) {
      case 1: // ATTR_ID
        v.setId(value);
        break;
      case 11: // ATTR_VISIBILITY
        v.setVisibility(value);
        break;
      case 12: // ATTR_ENABLED
        v.setEnabled(value != 0);
        break;
      case 13: // ATTR_FOCUSABLE
        v.setFocusable(value != 0);
        break;
      case 14: // ATTR_ALPHA
        v.setAlpha(Float.intBitsToFloat(value));
        break;
      case 15: // ATTR_TEXT
        setText(v, string(value));
        break;
      case 16: // ATTR_TEXT_COLOR
        if (v instanceof TextView) {
          ((TextView) v).setTextColor(value);
        }
        break;
      case 17: // ATTR_HINT
        if (v instanceof EditText) {
          ((EditText) v).setHint(string(value));
        }
        break;
      case 18: // ATTR_SINGLE_LINE
        if (v instanceof TextView) {
          ((TextView) v).setSingleLine(value != 0);
        }
        break;
      case 19: // ATTR_MAX_LINES
        if (v instanceof TextView) {
          ((TextView) v).setMaxLines(value);
        }
        break;
      case 20: // ATTR_ELLIPSIZE
        if (v instanceof TextView) {
          ((TextView) v).setEllipsize(truncateAt(value));
        }
        break;
      case 21: // ATTR_ORIENTATION
        if (v instanceof LinearLayout) {
          ((LinearLayout) v).setOrientation(value);
        }
        break;
      case 22: // ATTR_GRAVITY
        if (v instanceof LinearLayout) {
          ((LinearLayout) v).setGravity(value);
        } else if (v instanceof TextView) {
          ((TextView) v).setGravity(value);
        }
        break;
      case 23: // ATTR_SRC
        if (v instanceof ImageView) {
          ((ImageView) v).setImageResource(value);
        }
        break;
      case 24: // ATTR_SCALE_TYPE
        if (v instanceof ImageView) {
          ((ImageView) v).setScaleType(value);
        }
        break;
      case 25: // ATTR_TINT
        if (v instanceof ImageView) {
          ((ImageView) v).setTint(value);
        } else if (v instanceof ProgressBar) {
          ((ProgressBar) v).setTint(value);
        }
        break;
      case 26: // ATTR_CHECKED
        if (v instanceof CompoundButton) {
          ((CompoundButton) v).setChecked(value != 0);
        }
        break;
      case 27: // ATTR_PROGRESS
        if (v instanceof SeekBar) {
          ((SeekBar) v).setProgress(value);
        } else if (v instanceof ProgressBar) {
          ((ProgressBar) v).setProgress(value);
        }
        break;
      case 28: // ATTR_MAX
        if (v instanceof SeekBar) {
          ((SeekBar) v).setMax(value);
        } else if (v instanceof ProgressBar) {
          ((ProgressBar) v).setMax(value);
        }
        break;
      case 29: // ATTR_INPUT_TYPE
        if (v instanceof EditText) {
          ((EditText) v).setInputType(value);
        }
        break;
      case 30: // ATTR_TEXT_ON
        if (v instanceof ToggleButton) {
          ((ToggleButton) v).setTextOn(string(value));
        }
        break;
      case 31: // ATTR_TEXT_OFF
        if (v instanceof ToggleButton) {
          ((ToggleButton) v).setTextOff(string(value));
        }
        break;
      case 32: // ATTR_MIN
        if (v instanceof ProgressBar) {
          ((ProgressBar) v).setMin(value);
        }
        break;
      case 33: // ATTR_PROGRESS_TINT
        if (v instanceof ProgressBar) {
          ((ProgressBar) v).setProgressTintList(ColorStateList.valueOf(value));
        }
        break;
      case 34: // ATTR_PROGRESS_BACKGROUND_TINT
        if (v instanceof ProgressBar) {
          ((ProgressBar) v).setProgressBackgroundTintList(ColorStateList.valueOf(value));
        }
        break;
      case 35: // ATTR_INDETERMINATE_TINT
        if (v instanceof ProgressBar) {
          ((ProgressBar) v).setIndeterminateTintList(ColorStateList.valueOf(value));
        }
        break;
      case 36: // ATTR_INDICATOR_COLOR
        if (v instanceof CircularProgressIndicator) {
          ((CircularProgressIndicator) v).setIndicatorColor(value);
        }
        break;
      case 37: // ATTR_TRACK_COLOR
        if (v instanceof CircularProgressIndicator) {
          ((CircularProgressIndicator) v).setTrackColor(value);
        }
        break;
      case 38: // ATTR_TRACK_THICKNESS
        if (v instanceof CircularProgressIndicator) {
          ((CircularProgressIndicator) v).setTrackThickness(value);
        }
        break;
      case 39: // ATTR_INDICATOR_SIZE
        if (v instanceof CircularProgressIndicator) {
          ((CircularProgressIndicator) v).setIndicatorSize(value);
        }
        break;
      case 40: // ATTR_START_ANGLE
        if (v instanceof CircularProgressIndicator) {
          ((CircularProgressIndicator) v).setStartAngle(Float.intBitsToFloat(value));
        }
        break;
      case 41: // ATTR_SWEEP_ANGLE
        if (v instanceof CircularProgressIndicator) {
          ((CircularProgressIndicator) v).setSweepAngle(Float.intBitsToFloat(value));
        }
        break;
      case 42: // ATTR_TEXT_SIZE
        if (v instanceof TextView) {
          ((TextView) v).setTextSize(TypedValue.COMPLEX_UNIT_PX, Float.intBitsToFloat(value));
        }
        break;
      case 47: // ATTR_INCLUDE_FONT_PADDING
        if (v instanceof TextView) {
          ((TextView) v).setIncludeFontPadding(value != 0);
        }
        break;
      case 51: // ATTR_CLASS_NAME
        // Read by one() before the view exists; never reaches here.
        break;
      case 52: // ATTR_KEEP_SCREEN_ON
        v.setKeepScreenOn(value != 0);
        break;
      default:
        // An attribute from a newer compiler: skip it rather than fail the whole screen.
        break;
    }
  }

  private String string(int id) {
    return mContext.getResources().getString(id);
  }

  /** setText has no common declaring class: EditText and the compound buttons are not TextViews. */
  private static void setText(View v, String text) {
    if (v instanceof TextView) {
      ((TextView) v).setText(text);
    } else if (v instanceof EditText) {
      ((EditText) v).setText(text);
    } else if (v instanceof CheckBox) {
      ((CheckBox) v).setText(text);
    } else if (v instanceof RadioButton) {
      ((RadioButton) v).setText(text);
    }
  }

  private static TextUtils.TruncateAt truncateAt(int kind) {
    switch (kind) {
      case 1:
        return TextUtils.TruncateAt.START;
      case 2:
        return TextUtils.TruncateAt.MIDDLE;
      case 3:
        return TextUtils.TruncateAt.END;
      case 4:
        return TextUtils.TruncateAt.MARQUEE;
      default:
        return null;
    }
  }
}
