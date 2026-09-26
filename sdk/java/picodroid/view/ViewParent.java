// SPDX-License-Identifier: GPL-3.0-only
package picodroid.view;

/**
 * Mirrors {@code android.view.ViewParent}: what {@link View#getParent} returns. {@link ViewGroup}
 * is the one implementation, so Android's {@code ((ViewGroup) v.getParent()).removeView(v)} works
 * as written. Android's interface also carries the layout-request and focus-traversal hooks;
 * picodroid's layout is LVGL's, so none of them exist here yet.
 */
public interface ViewParent {}
