// SPDX-License-Identifier: GPL-3.0-only
package picoenvmonkt.ui.common

import picodroid.app.Activity
import picodroid.graphics.Theme
import picodroid.widget.LinearLayout

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
    /** Build the standard full-screen vertical root (themed background + padding). */
    protected fun makeScreenRoot(): LinearLayout {
        val root = LinearLayout()
        root.setOrientation(LinearLayout.VERTICAL)
        root.setSize(240, 240)
        root.setPadding(8, 6, 8, 6)
        root.setBackgroundColor(Theme.colorBackground)
        return root
    }

    /** Append the standardized A/B/X/Y button legend to the bottom of `root`. */
    protected fun installHintBar(root: LinearLayout, hints: String) {
        root.addView(createHintBar(hints))
    }
}
