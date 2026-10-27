// SPDX-License-Identifier: GPL-3.0-only
package picodroid.app;

import picodroid.view.View;
import picodroid.widget.Toolbar;

/**
 * Mirrors {@code android.app.ActionBar}: an Activity's app bar, which {@link Activity#getActionBar}
 * returns once a {@link Toolbar} has been made it with {@link Activity#setActionBar}. It shows the
 * Activity's title, an up button while {@link #DISPLAY_HOME_AS_UP} is set — a tap reaches {@link
 * Activity#onOptionsItemSelected} as {@code picodroid.R.id.home}, then {@link
 * Activity#onNavigateUp} — and the options menu behind its overflow button.
 *
 * <p>Divergences: there is no window-decor action bar, so {@code getActionBar()} is {@code null}
 * until {@code setActionBar} (Android's {@code NoActionBar} themes behave the same); no tabs,
 * custom views, logos or icons. The class is concrete with no public constructor, where Android's
 * is abstract.
 */
public class ActionBar {
  /** Mirrors Android: show the up affordance at the start of the bar. */
  public static final int DISPLAY_HOME_AS_UP = 0x4;

  /** Mirrors Android: show the title (and subtitle). */
  public static final int DISPLAY_SHOW_TITLE = 0x8;

  private final Activity mActivity;
  private final Toolbar mToolbar;
  private int mOptions = DISPLAY_SHOW_TITLE;
  private CharSequence mTitle;

  /** The up button's action, made once. */
  private final View.OnClickListener mUp;

  ActionBar(Activity activity, Toolbar toolbar) {
    mActivity = activity;
    mToolbar = toolbar;
    mUp = v -> mActivity.navigateHome();
  }

  /** Mirrors Android: show (or hide) the up button. */
  public void setDisplayHomeAsUpEnabled(boolean showHomeAsUp) {
    setDisplayOptions(showHomeAsUp ? DISPLAY_HOME_AS_UP : 0, DISPLAY_HOME_AS_UP);
  }

  /** Mirrors Android: show (or hide) the title and subtitle. */
  public void setDisplayShowTitleEnabled(boolean showTitle) {
    setDisplayOptions(showTitle ? DISPLAY_SHOW_TITLE : 0, DISPLAY_SHOW_TITLE);
  }

  /** Mirrors Android: the {@code DISPLAY_*} flags, all at once. */
  public void setDisplayOptions(int options) {
    mOptions = options;
    mToolbar.setNavigationOnClickListener((options & DISPLAY_HOME_AS_UP) != 0 ? mUp : null);
    mToolbar.setTitle((options & DISPLAY_SHOW_TITLE) != 0 ? mTitle : null);
  }

  /**
   * Mirrors Android: the {@code DISPLAY_*} flags in {@code mask}, set to those in {@code options}.
   */
  public void setDisplayOptions(int options, int mask) {
    setDisplayOptions((mOptions & ~mask) | (options & mask));
  }

  /** Mirrors Android: the {@code DISPLAY_*} flags in effect. */
  public int getDisplayOptions() {
    return mOptions;
  }

  /** Mirrors Android: the bar's title; the Activity's own title is left as it is. */
  public void setTitle(CharSequence title) {
    mTitle = title;
    if ((mOptions & DISPLAY_SHOW_TITLE) != 0) {
      mToolbar.setTitle(title);
    }
  }

  /** Mirrors Android: the title from the string resource {@code resId}. */
  public void setTitle(int resId) {
    setTitle(mActivity.getString(resId));
  }

  /** Mirrors Android: the bar's title. */
  public CharSequence getTitle() {
    return mTitle;
  }

  /** Mirrors Android: a second line under the title, or {@code null}. */
  public void setSubtitle(CharSequence subtitle) {
    mToolbar.setSubtitle(subtitle);
  }

  /** Mirrors Android: the subtitle from the string resource {@code resId}. */
  public void setSubtitle(int resId) {
    mToolbar.setSubtitle(resId);
  }

  /** Mirrors Android: the subtitle, or {@code null}. */
  public CharSequence getSubtitle() {
    return mToolbar.getSubtitle();
  }

  /** Mirrors Android: show the bar; it takes its place in the layout again. */
  public void show() {
    mToolbar.setVisibility(View.VISIBLE);
  }

  /** Mirrors Android: hide the bar; the layout closes up over it. */
  public void hide() {
    mToolbar.setVisibility(View.GONE);
  }

  /** Mirrors Android: whether the bar shows. */
  public boolean isShowing() {
    return mToolbar.getVisibility() == View.VISIBLE;
  }

  /** Mirrors Android: the bar's height in pixels. */
  public int getHeight() {
    return mToolbar.getHeight();
  }

  /** The Toolbar this bar is. */
  Toolbar toolbar() {
    return mToolbar;
  }
}
