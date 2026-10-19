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
import picodroid.widget.GridLayout;
import picodroid.widget.ImageView;
import picodroid.widget.InflaterSupport;
import picodroid.widget.LinearLayout;
import picodroid.widget.ListView;
import picodroid.widget.ProgressBar;
import picodroid.widget.RadioButton;
import picodroid.widget.RadioGroup;
import picodroid.widget.RelativeLayout;
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

  // What nativeInflateNode leaves for one() after making a node: its layout_* words, for the
  // LayoutParams, and how many (attribute, value) pairs it handed back in mDeferred for apply().
  // Written from the runtime by field slot (graphics/fields.rs): append only.
  private int mWidth;
  private int mHeight;
  private float mWeight;
  private int mLayoutGravity;
  private int mMarginLeft;
  private int mMarginTop;
  private int mMarginRight;
  private int mMarginBottom;
  private int mDeferredCount;
  private int[] mDeferred;

  /**
   * How one() made its last view: through the native path, so its widget is already under its
   * parent's and sized; and whether it is a class of the app's own, whose addView must be the one
   * to run for its children.
   */
  private boolean mFast;

  private boolean mCustom;

  // A GridLayout child's cell, from its layout_column/row/…Span/…Weight words, for params();
  // mGridAttrs says the node carried any. Reset once the params are built.
  private int mColumn = GridLayout.UNDEFINED;
  private int mRow = GridLayout.UNDEFINED;
  private int mColumnSpan = 1;
  private int mRowSpan = 1;
  private float mColumnWeight;
  private float mRowWeight;
  private boolean mGridAttrs;

  /** The (attribute, value) pairs nativeInflateNode may hand back per node, at most. */
  private static final int MAX_DEFERRED = 8;

  /** The kinds of parent nativeInflateNode lays a child out for (lvgl/inflate.rs ParentKind). */
  private static final int PARENT_NONE = 0;

  private static final int PARENT_LINEAR = 1;
  private static final int PARENT_FRAME = 2;
  private static final int PARENT_OTHER = 3;
  private static final int PARENT_CUSTOM = 4;
  private static final int PARENT_GRID = 5;
  private static final int PARENT_RELATIVE = 6;

  private LayoutInflater(Context context) {
    mContext = context;
    if (context instanceof Factory) {
      mFactory = (Factory) context; // an Activity makes its own custom views, as on Android
    }
    mDeferred = new int[2 * MAX_DEFERRED];
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
    boolean attach = root != null && attachToRoot;
    View view = node(root, attach ? PARENT_CUSTOM : PARENT_NONE);
    if (attach) {
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
      mOpenKind = new int[8];
    }
  }

  /** The parent kind of each open group, beside {@link #mOpen}. */
  private int[] mOpenKind;

  // A RelativeLayout child's rules, for params(): the boolean verbs as one word and the id each
  // sibling verb names, by the runtime's slots. Written from the runtime by field slot
  // (graphics/fields.rs) for a child of a RelativeLayout: append only, after every other field.
  private int mRelativeFlags;
  private int mAnchor0;
  private int mAnchor1;
  private int mAnchor2;
  private int mAnchor3;
  private int mAnchor4;
  private int mAnchor5;
  private int mAnchor6;
  private int mAnchor7;

  /**
   * Builds views until the tree is whole or {@code SystemClock.elapsedRealtime()} reaches {@code
   * deadlineMs}, always at least one. Returns whether there is more to build.
   */
  boolean resume(long deadlineMs) {
    do {
      boolean top = mDepth == 0;
      ViewGroup parent = top ? mRootParent : mOpen[mDepth - 1];
      int kind = top ? PARENT_NONE : mOpenKind[mDepth - 1];
      View v = one(parent, kind);
      int children = mChildCount;
      boolean custom = mCustom;
      if (top) {
        mRoot = v;
        // The tree is built where every new view is, on the screen, over several frames: keep
        // it out of sight until it is whole and its owner has put it where it belongs.
        mRootVisibility = v.getVisibility();
        v.setVisibility(View.INVISIBLE);
      } else if (mFast && kind != PARENT_CUSTOM && kind != PARENT_GRID) {
        parent.attachInflated(v, v.layoutParams); // see node()
        mOpenLeft[mDepth - 1]--;
      } else {
        parent.addView(v, v.getLayoutParams());
        mOpenLeft[mDepth - 1]--;
      }
      if (children > 0) {
        if (mDepth == mOpen.length) {
          ViewGroup[] open = new ViewGroup[mDepth * 2];
          int[] left = new int[mDepth * 2];
          int[] kinds = new int[mDepth * 2];
          System.arraycopy(mOpen, 0, open, 0, mDepth);
          System.arraycopy(mOpenLeft, 0, left, 0, mDepth);
          System.arraycopy(mOpenKind, 0, kinds, 0, mDepth);
          mOpen = open;
          mOpenLeft = left;
          mOpenKind = kinds;
        }
        ViewGroup group = (ViewGroup) v; // the compiler only nests under ViewGroup classes
        mOpen[mDepth] = group;
        mOpenLeft[mDepth] = children;
        mOpenKind[mDepth] = kindOf(group, custom);
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

  /**
   * Makes the framework view at the read position in one call: the runtime reads the node's words,
   * creates {@code v}'s widget under {@code parentHandle}'s (the screen for 0), applies every
   * attribute it has a setter for under one style refresh, lays it out as {@code parentKind}'s
   * {@code addView} would, and writes {@code v}'s handle and cached fields, {@link #mPos}, {@link
   * #mChildCount} and the layout_* fields. The attributes it leaves to {@link #apply} land in
   * {@code deferred}, {@link #mDeferredCount} pairs. Returns false, with nothing made and the
   * position untouched, for a node that takes the Java path (a class it does not cover, a plain
   * View to measure, more attributes to hand back than fit, a full widget pool).
   */
  private native boolean nativeInflateNode(
      View v, int layout, int pos, int parentHandle, int parentKind, int[] deferred);

  private int next() {
    return nativeWord(mLayout, mPos++);
  }

  /**
   * Inflates the node at the read position and everything under it; with a {@code kind} other than
   * {@link #PARENT_NONE}, joins it to {@code parent}, which it was made under or is moved under.
   */
  private View node(ViewGroup parent, int kind) {
    View v = one(parent, kind);
    int children = mChildCount;
    boolean custom = mCustom;
    // A view the native path made is already under the parent's widget and laid out, so only
    // the Java side joins; one from the Java path, under a group of the app's own, or under a
    // GridLayout (which places it in its cell) goes through addView, so an override of it runs.
    // With PARENT_NONE the caller places it.
    if (kind == PARENT_NONE) {
      // Not joined here.
    } else if (mFast && kind != PARENT_CUSTOM && kind != PARENT_GRID) {
      parent.attachInflated(v, v.layoutParams);
    } else {
      parent.addView(v, v.getLayoutParams());
    }
    if (children > 0) {
      // The compiler only nests under the ViewGroup classes create() knows.
      ViewGroup group = (ViewGroup) v;
      int childKind = kindOf(group, custom);
      for (int i = 0; i < children; i++) {
        node(group, childKind);
      }
    }
    return v;
  }

  /** How {@code group} lays its children out, for nativeInflateNode. */
  private static int kindOf(ViewGroup group, boolean custom) {
    if (custom) {
      return PARENT_CUSTOM;
    }
    if (group instanceof LinearLayout) {
      return PARENT_LINEAR;
    }
    if (group instanceof FrameLayout) {
      return PARENT_FRAME;
    }
    if (group instanceof GridLayout) {
      return PARENT_GRID;
    }
    if (group instanceof RelativeLayout) {
      return PARENT_RELATIVE;
    }
    return PARENT_OTHER;
  }

  /**
   * Makes the one view at the read position, its LayoutParams made for {@code parent}, and leaves
   * the number of children that follow it in {@link #mChildCount}: through the native path for the
   * framework classes it covers, else through {@link #oneLegacy}. {@code kind} says how the native
   * path should lay the view out for its parent, {@link #PARENT_NONE} for a view that is not joined
   * to one.
   */
  private View one(ViewGroup parent, int kind) {
    int cls = nativeWord(mLayout, mPos) & 0xFF;
    View v = cls == 20 ? View.plain() : InflaterSupport.adopt(cls);
    if (v != null) {
      v.mContext = mContext;
    }
    // A parent whose widget was freed has handle 0, which would read as "the screen" to the
    // native path: the Java path's addView throws for it instead.
    int parentHandle = kind == PARENT_NONE ? 0 : parent.nativeHandle;
    if (v != null
        && (kind == PARENT_NONE || parentHandle != 0)
        && nativeInflateNode(v, mLayout, mPos, parentHandle, kind, mDeferred)) {
      mFast = true;
      mCustom = false;
      // The deferred words first: a grid child's cell is among them, and params() reads it.
      if (mDeferredCount != 0) {
        applyDeferred(v);
      }
      v.layoutParams =
          params(
              parent,
              mWidth,
              mHeight,
              mWeight,
              mLayoutGravity,
              mMarginLeft,
              mMarginTop,
              mMarginRight,
              mMarginBottom);
      return v;
    }
    return oneLegacy(parent);
  }

  /** The attributes {@link #nativeInflateNode} handed back: a {@code <shape>}, and the rest. */
  private void applyDeferred(View v) {
    int n = mDeferredCount;
    boolean filled = false;
    int fill = 0;
    int radius = 0;
    int strokeWidth = 0;
    int strokeColor = 0;
    for (int i = 0; i < n; i++) {
      int attr = mDeferred[2 * i];
      int value = mDeferred[2 * i + 1];
      if (attr == 10) { // the background word of a <shape>
        fill = value;
        filled = true;
      } else if (attr == 48) { // its radius
        radius = value;
      } else if (attr == 49) { // its stroke width
        strokeWidth = value;
      } else if (attr == 50) { // its stroke colour
        strokeColor = value;
      } else {
        apply(v, attr, value);
      }
    }
    background(v, filled, fill, radius, strokeWidth, strokeColor);
  }

  /** The LayoutParams {@code parent} wants, from a node's layout_* words. */
  private ViewGroup.MarginLayoutParams params(
      ViewGroup parent,
      int width,
      int height,
      float weight,
      int layoutGravity,
      int marginLeft,
      int marginTop,
      int marginRight,
      int marginBottom) {
    ViewGroup.MarginLayoutParams lp;
    if (parent instanceof LinearLayout) {
      LinearLayout.LayoutParams llp = new LinearLayout.LayoutParams(width, height, weight);
      llp.gravity = layoutGravity;
      lp = llp;
    } else if (parent instanceof FrameLayout) {
      lp = new FrameLayout.LayoutParams(width, height, layoutGravity);
    } else if (parent instanceof GridLayout) {
      GridLayout.LayoutParams glp =
          new GridLayout.LayoutParams(
              GridLayout.spec(mRow, mRowSpan, (GridLayout.Alignment) null, mRowWeight),
              GridLayout.spec(mColumn, mColumnSpan, (GridLayout.Alignment) null, mColumnWeight));
      glp.width = width;
      glp.height = height;
      if (layoutGravity != 0) {
        glp.setGravity(layoutGravity);
      }
      lp = glp;
    } else if (parent instanceof RelativeLayout) {
      lp = relativeParams(width, height);
    } else {
      lp = new ViewGroup.MarginLayoutParams(width, height);
    }
    if (mGridAttrs) {
      mColumn = GridLayout.UNDEFINED;
      mRow = GridLayout.UNDEFINED;
      mColumnSpan = 1;
      mRowSpan = 1;
      mColumnWeight = 0;
      mRowWeight = 0;
      mGridAttrs = false;
    }
    if ((marginLeft | marginTop | marginRight | marginBottom) != 0) {
      lp.setMargins(marginLeft, marginTop, marginRight, marginBottom);
    }
    return lp;
  }

  /** The boolean verbs by their bit in the rules word (papk_format's LAYOUT_RELATIVE_FLAGS). */
  private static final int[] RELATIVE_FLAG_VERBS = {
    RelativeLayout.ALIGN_PARENT_LEFT,
    RelativeLayout.ALIGN_PARENT_TOP,
    RelativeLayout.ALIGN_PARENT_RIGHT,
    RelativeLayout.ALIGN_PARENT_BOTTOM,
    RelativeLayout.CENTER_IN_PARENT,
    RelativeLayout.CENTER_HORIZONTAL,
    RelativeLayout.CENTER_VERTICAL,
  };

  /** The sibling verbs by their anchor slot (the runtime's order). */
  private static final int[] RELATIVE_ANCHOR_VERBS = {
    RelativeLayout.ABOVE,
    RelativeLayout.BELOW,
    RelativeLayout.LEFT_OF,
    RelativeLayout.RIGHT_OF,
    RelativeLayout.ALIGN_LEFT,
    RelativeLayout.ALIGN_TOP,
    RelativeLayout.ALIGN_RIGHT,
    RelativeLayout.ALIGN_BOTTOM,
  };

  /** The RelativeLayout.LayoutParams a node's rule words say, and the words cleared. */
  private RelativeLayout.LayoutParams relativeParams(int width, int height) {
    RelativeLayout.LayoutParams lp = new RelativeLayout.LayoutParams(width, height);
    int flags = mRelativeFlags;
    for (int bit = 0; bit < RELATIVE_FLAG_VERBS.length; bit++) {
      if ((flags & (1 << bit)) != 0) {
        lp.addRule(RELATIVE_FLAG_VERBS[bit]);
      }
    }
    lp.alignWithParent = (flags & (1 << 7)) != 0;
    int[] anchors = {
      mAnchor0, mAnchor1, mAnchor2, mAnchor3, mAnchor4, mAnchor5, mAnchor6, mAnchor7
    };
    for (int slot = 0; slot < anchors.length; slot++) {
      if (anchors[slot] != 0) {
        lp.addRule(RELATIVE_ANCHOR_VERBS[slot], anchors[slot]);
      }
    }
    mRelativeFlags = 0;
    mAnchor0 = 0;
    mAnchor1 = 0;
    mAnchor2 = 0;
    mAnchor3 = 0;
    mAnchor4 = 0;
    mAnchor5 = 0;
    mAnchor6 = 0;
    mAnchor7 = 0;
    return lp;
  }

  /** A node's background: a {@code <shape>} the compiler flattened, or a colour, or nothing. */
  private static void background(
      View v, boolean filled, int fill, int radius, int strokeWidth, int strokeColor) {
    if (radius != 0 || strokeWidth != 0) {
      GradientDrawable shape = new GradientDrawable().setColor(fill).setCornerRadius(radius);
      if (strokeWidth != 0) {
        shape.setStroke(strokeWidth, strokeColor);
      }
      v.setBackground(shape);
    } else if (filled) {
      v.setBackgroundColor(fill);
    }
  }

  /**
   * {@link #one} through the Java setters: a view class of the app's own, made by the {@link
   * Factory} or by reflection, or a framework class the native path does not cover.
   */
  private View oneLegacy(ViewGroup parent) {
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
    mFast = false;
    mCustom = cls == 19;

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
        case 64: // ATTR_LAYOUT_RELATIVE_FLAGS
          mRelativeFlags = value;
          break;
        case 65: // ATTR_LAYOUT_ABOVE
          mAnchor0 = value;
          break;
        case 66: // ATTR_LAYOUT_BELOW
          mAnchor1 = value;
          break;
        case 67: // ATTR_LAYOUT_TO_LEFT_OF
          mAnchor2 = value;
          break;
        case 68: // ATTR_LAYOUT_TO_RIGHT_OF
          mAnchor3 = value;
          break;
        case 69: // ATTR_LAYOUT_ALIGN_LEFT
          mAnchor4 = value;
          break;
        case 70: // ATTR_LAYOUT_ALIGN_TOP
          mAnchor5 = value;
          break;
        case 71: // ATTR_LAYOUT_ALIGN_RIGHT
          mAnchor6 = value;
          break;
        case 72: // ATTR_LAYOUT_ALIGN_BOTTOM
          mAnchor7 = value;
          break;
        default:
          apply(v, attr, value);
          break;
      }
    }
    if (padded) {
      v.setPadding(padLeft, padTop, padRight, padBottom);
    }
    background(v, filled, fill, radius, strokeWidth, strokeColor);

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

    v.setLayoutParams(
        params(
            parent,
            width,
            height,
            weight,
            layoutGravity,
            marginLeft,
            marginTop,
            marginRight,
            marginBottom));
    return v;
  }

  /**
   * A view of the app's own, by class name: the {@link Factory} first (an Activity's {@code
   * onCreateView}, which returns null unless overridden), else the class's {@code (Context,
   * AttributeSet)} constructor by reflection, as Android does.
   */
  private View custom(String name) {
    View v = mFactory == null ? null : mFactory.onCreateView(name, mContext, NO_ATTRIBUTES);
    if (v != null) {
      return v;
    }
    Class<?> cls;
    try {
      cls = Class.forName(name);
    } catch (ClassNotFoundException e) {
      throw new InflateException("Error inflating class " + name + ": class not found");
    }
    v = nativeNewView(cls, mContext, NO_ATTRIBUTES);
    if (v == null) {
      throw new InflateException(
          "Error inflating class " + name + ": no (Context, AttributeSet) constructor");
    }
    return v;
  }

  /**
   * {@code new <cls>(context, attrs)}, or null when {@code cls} is not a concrete class with that
   * constructor. The interpreter runs the constructor as a frame (there is no {@code
   * java.lang.reflect.Constructor}).
   */
  private static native View nativeNewView(Class<?> cls, Context context, AttributeSet attrs);

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
      case 21: // CLASS_SPACE
        return new picodroid.widget.Space(mContext);
      case 22: // CLASS_GRID_LAYOUT
        return new GridLayout(mContext);
      case 23: // CLASS_RELATIVE_LAYOUT
        return new RelativeLayout(mContext);
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
        } else if (v instanceof GridLayout) {
          ((GridLayout) v).setOrientation(value);
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
      case 53: // ATTR_MIN_WIDTH
        v.setMinimumWidth(value);
        break;
      case 54: // ATTR_MIN_HEIGHT
        v.setMinimumHeight(value);
        break;
      case 55: // ATTR_MAX_WIDTH
        if (v instanceof TextView) {
          ((TextView) v).setMaxWidth(value);
        }
        break;
      case 56: // ATTR_LAYOUT_COLUMN
        mColumn = value;
        mGridAttrs = true;
        break;
      case 57: // ATTR_LAYOUT_ROW
        mRow = value;
        mGridAttrs = true;
        break;
      case 58: // ATTR_LAYOUT_COLUMN_SPAN
        mColumnSpan = value;
        mGridAttrs = true;
        break;
      case 59: // ATTR_LAYOUT_ROW_SPAN
        mRowSpan = value;
        mGridAttrs = true;
        break;
      case 60: // ATTR_LAYOUT_COLUMN_WEIGHT
        mColumnWeight = Float.intBitsToFloat(value);
        mGridAttrs = true;
        break;
      case 61: // ATTR_LAYOUT_ROW_WEIGHT
        mRowWeight = Float.intBitsToFloat(value);
        mGridAttrs = true;
        break;
      case 62: // ATTR_COLUMN_COUNT
        if (v instanceof GridLayout) {
          ((GridLayout) v).setColumnCount(value);
        }
        break;
      case 63: // ATTR_ROW_COUNT
        if (v instanceof GridLayout) {
          ((GridLayout) v).setRowCount(value);
        }
        break;
      default:
        // An attribute from a newer compiler: skip it rather than fail the whole screen.
        break;
    }
  }

  private String string(int id) {
    return mContext.getResources().getString(id);
  }

  /** setText has no common declaring class: the compound buttons are not TextViews. */
  private static void setText(View v, String text) {
    if (v instanceof TextView) {
      ((TextView) v).setText(text);
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
