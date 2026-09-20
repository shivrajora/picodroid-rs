// SPDX-License-Identifier: GPL-3.0-only
package injectdemokt

import picodroid.os.Bundle
import picodroid.util.Log
import picodroid.widget.TextView

/**
 * Declares no @Inject members of its own — the generated leaf injector delegates to BaseActivity's.
 * (A `lateinit` read that was never injected throws, so the token line is still a proof.)
 */
class DetailActivity : BaseActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        Log.i(TAG, "Detail clock#${clock.id} inherited=true")
        getDisplay()
        val text = TextView()
        text.setText("detail clock#${clock.id}")
        setContentView(text)
    }
}
