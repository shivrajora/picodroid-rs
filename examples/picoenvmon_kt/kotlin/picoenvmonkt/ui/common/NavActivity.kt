// SPDX-License-Identifier: GPL-3.0-only
package picoenvmonkt.ui.common

import picodroid.app.Activity
import picodroid.graphics.Theme
import picodroid.widget.LinearLayout
import picodroid.widget.TextView
import picoenvmon_kt.R

/**
 * Base for every picoenvmon screen under the standardized 4-button navigation model:
 * - **A** = up (previous focusable)
 * - **B** = down (next focusable)
 * - **X** = open / activate the focused item
 * - **Y** = back
 *
 * Navigation itself is handled natively by the per-Activity keypad focus group — PREV/NEXT move the
 * focus highlight between focusable widgets, ENTER clicks the focused widget, and ESC runs the back
 * chain (dismiss keyboard/dialog, then [Activity.onBackPressed]). So this base does not implement
 * an `OnKeyListener`; it only standardizes the screen frame and the always-visible button legend,
 * removing the per-screen focus/key boilerplate every screen used to repeat. Subclasses just build
 * their content with focusable widgets (Buttons, ListView rows, EditTexts, Switches) and the four
 * buttons behave identically everywhere. `abstract` because Kotlin classes are final by default.
 */
abstract class NavActivity : Activity() {
    /**
     * Build the standard vertical root (themed background + padding). As the content view it fills
     * the panel, whichever board this runs on: the app was first written against the Enviro+'s
     * 240x240 and must look the same on a wider or taller one.
     */
    protected fun makeScreenRoot(): LinearLayout {
        val root = LinearLayout(this)
        root.setOrientation(LinearLayout.VERTICAL)
        // A live monitor is watched, not touched: hold the panel on while a screen shows.
        root.setKeepScreenOn(true)
        val padH = dimen(R.dimen.screen_pad_h)
        val padV = dimen(R.dimen.screen_pad_v)
        root.setPadding(padH, padV, padH, padV)
        root.setBackgroundColor(Theme.colorBackground)
        return root
    }

    /** A title line in the primary colour, appended to `root`. */
    protected fun addTitle(root: LinearLayout, textRes: Int) {
        val title = TextView(this)
        title.setText(getString(textRes))
        title.setTextColor(Theme.colorPrimary)
        root.addView(title)
    }

    /** Append the standardized A/B/X/Y legend to the bottom of `root`, `topMargin` below. */
    protected fun installHintBar(root: LinearLayout, hintsRes: Int, topMargin: Int = 0) {
        root.addView(
            createHintBar(this, getString(hintsRes)),
            stacked(dimen(R.dimen.hint_bar_height), topMargin),
        )
    }

    /** Layout params for a child of a vertical screen: full width, `height`, a top margin. */
    protected fun stacked(height: Int, topMargin: Int): LinearLayout.LayoutParams {
        val lp = LinearLayout.LayoutParams(LinearLayout.LayoutParams.MATCH_PARENT, height)
        lp.topMargin = topMargin
        return lp
    }

    /** A pixel size from `res/values/dimens.xml`. */
    protected fun dimen(dimenRes: Int): Int = getResources().getDimensionPixelSize(dimenRes)
}
