// SPDX-License-Identifier: GPL-3.0-only
package picodroid.widget;

import picodroid.annotation.Hide;
import picodroid.content.Context;
import picodroid.text.TextUtils;
import picodroid.view.View;
import picodroid.view.ViewGroup;

/**
 * Mirrors {@code android.widget.Toolbar}: the app bar at the top of a screen — a navigation button
 * at the start, the title (and a subtitle under it), and the overflow button at the end. Put it at
 * the top of the layout and hand it to {@link picodroid.app.Activity#setActionBar}: it then shows
 * the Activity's title (the manifest's {@code android:label}, or {@code setTitle}), the up button
 * once {@code getActionBar().setDisplayHomeAsUpEnabled(true)}, and the Activity's options menu
 * behind its overflow button. A Toolbar not made the action bar shows what it is told.
 *
 * <p>A Toolbar is {@link #DEFAULT_HEIGHT} pixels tall unless its layout params say otherwise, and
 * has no background of its own: give it one ({@code setBackgroundColor}), as an app gives Android's
 * its {@code colorPrimary}. Children an app adds are placed at the end, before the overflow button,
 * centred on the bar's height, at the size they measure.
 *
 * <p>Divergences: the navigation button is the stock back chevron and shows whenever a navigation
 * listener is set (there is no {@code setNavigationIcon}); the overflow is the Activity's options
 * menu, so a Toolbar has no menu of its own ({@code getMenu}, {@code inflateMenu}); its buttons are
 * touch targets, not focus stops — the keys reach the same actions through BACK and the menu key
 * (or a held SELECT); no {@code AttributeSet} constructor, so it is built in code.
 */
public class Toolbar extends ViewGroup {
  /** The bar's height when its layout params leave it to the bar, in pixels. */
  public static final int DEFAULT_HEIGHT = 40;

  /** Room between the navigation button and the title, or the bar's edge and the title. */
  private static final int TITLE_INSET = 8;

  /** {@code LV_SYMBOL_LEFT}, the back chevron, from the symbol range the default face carries. */
  private static final String GLYPH_NAVIGATION = "";

  /** {@code LV_SYMBOL_BARS}, what the framework's own menu control shows. */
  private static final String GLYPH_OVERFLOW = "";

  private Button mNavButton;
  private TextView mTitleView;
  private TextView mSubtitleView;
  private Button mOverflowButton;
  private CharSequence mTitle;
  private CharSequence mSubtitle;
  private int mTitleColor;
  private boolean mTitleColorSet;
  private View.OnClickListener mNavListener;
  private View.OnClickListener mOverflowListener;

  /** Mirrors Android: an empty bar; set a title and a navigation listener to fill it. */
  public Toolbar(Context context) {
    super(context);
  }

  /** Mirrors Android: the title, or {@code null} for none; one line, cut with an ellipsis. */
  public void setTitle(CharSequence title) {
    mTitle = title;
    boolean had = shows(mTitleView);
    if (TextUtils.isEmpty(title)) {
      if (mTitleView != null) {
        mTitleView.setVisibility(GONE);
      }
    } else {
      if (mTitleView == null) {
        mTitleView = line();
        if (mTitleColorSet) {
          mTitleView.setTextColor(mTitleColor);
        }
      }
      mTitleView.setText(title.toString());
      mTitleView.setVisibility(VISIBLE);
    }
    if (had != shows(mTitleView)) {
      requestLayout();
    }
  }

  /** Mirrors Android: the title from the string resource {@code resId}. */
  public void setTitle(int resId) {
    setTitle(getContext().getString(resId));
  }

  /** Mirrors Android: the title last set, or {@code null}. */
  public CharSequence getTitle() {
    return mTitle;
  }

  /** Mirrors Android: a second line under the title, or {@code null} for none. */
  public void setSubtitle(CharSequence subtitle) {
    boolean had = !TextUtils.isEmpty(mSubtitle);
    mSubtitle = subtitle;
    boolean has = !TextUtils.isEmpty(subtitle);
    if (has) {
      if (mSubtitleView == null) {
        mSubtitleView = line();
        if (mTitleColorSet) {
          mSubtitleView.setTextColor(mTitleColor);
        }
      }
      mSubtitleView.setText(subtitle.toString());
      mSubtitleView.setVisibility(VISIBLE);
    } else if (mSubtitleView != null) {
      mSubtitleView.setVisibility(GONE);
    }
    if (had != has) {
      requestLayout();
    }
  }

  /** Mirrors Android: the subtitle from the string resource {@code resId}. */
  public void setSubtitle(int resId) {
    setSubtitle(getContext().getString(resId));
  }

  /** Mirrors Android: the subtitle last set, or {@code null}. */
  public CharSequence getSubtitle() {
    return mSubtitle;
  }

  /** Mirrors Android: the title's colour. */
  public void setTitleTextColor(int color) {
    mTitleColor = color;
    mTitleColorSet = true;
    if (mTitleView != null) {
      mTitleView.setTextColor(color);
    }
  }

  /** Mirrors Android: the subtitle's colour. */
  public void setSubtitleTextColor(int color) {
    if (mSubtitleView == null) {
      mSubtitleView = line();
      mSubtitleView.setVisibility(GONE);
    }
    mSubtitleView.setTextColor(color);
  }

  /**
   * Mirrors Android: what a tap on the navigation button does. The button (a back chevron) shows
   * while a listener is set; {@code null} takes it away. On an action bar the Activity owns it: see
   * {@link picodroid.app.ActionBar#setDisplayHomeAsUpEnabled}.
   */
  public void setNavigationOnClickListener(View.OnClickListener listener) {
    mNavListener = listener;
    if (listener != null && mNavButton == null) {
      mNavButton = button(GLYPH_NAVIGATION, v -> fireNavigation());
      requestLayout();
    } else if (mNavButton != null) {
      boolean shown = mNavButton.getVisibility() == VISIBLE;
      mNavButton.setVisibility(listener != null ? VISIBLE : GONE);
      if (shown != (listener != null)) {
        requestLayout();
      }
    }
  }

  /** Mirrors Android: opens the overflow menu, if the bar has one; true when it opened. */
  public boolean showOverflowMenu() {
    if (mOverflowListener == null) {
      return false;
    }
    mOverflowListener.onClick(mOverflowButton);
    return true;
  }

  /**
   * The overflow button's action, or {@code null} for no button: what {@link
   * picodroid.app.Activity#setActionBar} wires to the options menu.
   */
  @Hide("Activity.setActionBar(Toolbar) and onCreateOptionsMenu")
  public void setOverflowOnClickListener(View.OnClickListener listener) {
    boolean had = mOverflowListener != null;
    mOverflowListener = listener;
    if (listener != null && mOverflowButton == null) {
      mOverflowButton = button(GLYPH_OVERFLOW, v -> showOverflowMenu());
    } else if (mOverflowButton != null) {
      mOverflowButton.setVisibility(listener != null ? VISIBLE : GONE);
    }
    if (had != (listener != null)) {
      requestLayout();
    }
  }

  private void fireNavigation() {
    if (mNavListener != null) {
      mNavListener.onClick(mNavButton);
    }
  }

  /** A single-line label cut with an ellipsis, added to the bar. */
  private TextView line() {
    TextView t = new TextView(getContext());
    t.setSingleLine();
    t.setEllipsize(TextUtils.TruncateAt.END);
    addView(t);
    return t;
  }

  /**
   * A square, borderless button showing {@code glyph}, added to the bar. Out of the focus ring:
   * BACK and the menu key already reach what it does.
   */
  private Button button(String glyph, View.OnClickListener onClick) {
    Button b = new Button(getContext());
    b.setText(glyph);
    b.setBackgroundColor(0);
    b.setOnClickListener(onClick);
    b.setFocusable(false);
    addView(b);
    return b;
  }

  @Override
  protected void onMeasure(int widthMeasureSpec, int heightMeasureSpec) {
    setMeasuredDimension(
        getDefaultSize(getSuggestedMinimumWidth(), widthMeasureSpec),
        MeasureSpec.getMode(heightMeasureSpec) == MeasureSpec.EXACTLY
            ? MeasureSpec.getSize(heightMeasureSpec)
            : DEFAULT_HEIGHT);
  }

  @Override
  protected void onLayout(boolean changed, int left, int top, int right, int bottom) {
    int w = right - left;
    int h = bottom - top;
    int start = TITLE_INSET;
    if (shows(mNavButton)) {
      mNavButton.layout(0, 0, h, h);
      start = h;
    }
    int end = w;
    if (shows(mOverflowButton)) {
      mOverflowButton.layout(w - h, 0, w, h);
      end = w - h;
    }
    // An app's own children go at the end, right to left, before the overflow button.
    int atMost = MeasureSpec.makeMeasureSpec(h, MeasureSpec.AT_MOST);
    for (int i = getChildCount() - 1; i >= 0; i--) {
      View child = getChildAt(i);
      if (isOwn(child) || child.getVisibility() == GONE) {
        continue;
      }
      measureChild(
          child,
          MeasureSpec.makeMeasureSpec(Math.max(0, end - start), MeasureSpec.AT_MOST),
          atMost);
      int cw = child.getMeasuredWidth();
      int ch = child.getMeasuredHeight();
      int y = (h - ch) / 2;
      child.layout(end - cw, y, end, y + ch);
      end -= cw;
    }
    int textWidth = Math.max(0, end - start - TITLE_INSET);
    boolean title = shows(mTitleView);
    boolean subtitle = shows(mSubtitleView);
    int titleHeight = title ? mTitleView.getLineHeight() : 0;
    int subtitleHeight = subtitle ? mSubtitleView.getLineHeight() : 0;
    int y = (h - titleHeight - subtitleHeight) / 2;
    if (title) {
      mTitleView.layout(start, y, start + textWidth, y + titleHeight);
      y += titleHeight;
    }
    if (subtitle) {
      mSubtitleView.layout(start, y, start + textWidth, y + subtitleHeight);
    }
  }

  private static boolean shows(View v) {
    return v != null && v.getVisibility() != GONE;
  }

  private boolean isOwn(View v) {
    return v == mNavButton || v == mTitleView || v == mSubtitleView || v == mOverflowButton;
  }
}
