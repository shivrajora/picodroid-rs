// SPDX-License-Identifier: GPL-3.0-only
@file:JvmName("ButtonHintBar")

package picoenvmonkt.ui.common

import picodroid.graphics.Theme
import picodroid.widget.LinearLayout
import picodroid.widget.TextView

/**
 * Always-visible on-screen legend mapping the four hardware buttons (A/B/X/Y) to their current
 * actions, so users of this touchless 4-button device always know what each button does. Built once
 * per screen by [NavActivity.installHintBar], replacing the old hand-written, drift-prone footer
 * strings.
 *
 * A factory function rather than a `View` subclass, to keep the bar a plain composed [LinearLayout]
 * (a label row), matching how the rest of the app builds UI.
 */

/** Build a single-line legend row, e.g. `"A:Up B:Down X:Open Y:Back"`. */
fun createHintBar(hints: String): LinearLayout {
    val bar = LinearLayout()
    bar.setOrientation(LinearLayout.HORIZONTAL)
    bar.setSize(224, 18)

    val label = TextView()
    label.setText(hints)
    label.setTextColor(Theme.colorTextSecondary)
    bar.addView(label)

    return bar
}
