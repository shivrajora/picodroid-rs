// SPDX-License-Identifier: GPL-3.0-only
package picoenvmonkt

import javax.inject.Inject
import picodroid.app.Application
import picodroid.content.Context
import picodroid.content.Intent
import picodroid.graphics.Theme
import picoenvmon_kt.R
import picoenvmonkt.data.ThresholdConfig
import picoenvmonkt.net.NetworkManager
import picoenvmonkt.ui.home.HomeActivity

/**
 * App entry point. The object graph is wired by `@Inject`/`@Singleton`
 * (docs/designs/inject-annotations-2026-08.md), processed by kapt: the framework injects this
 * Application's fields before `onCreate`, and every Activity and Service below gets the same
 * treatment, so the singletons ([NetworkManager], ThresholdConfig, Formatter, LatestReadings,
 * RgbLed) are shared without a hand-written component. What needs a Context gets this one, the
 * application context, as an argument.
 */
class EnvApp : Application() {
    /** Owns the network callback, dashboard server, clock watch and weather refresh. */
    @Inject lateinit var networkManager: NetworkManager

    @Inject lateinit var thresholds: ThresholdConfig

    override fun onCreate() {
        Theme.colorBackground = getColor(R.color.background)
        Theme.colorSurface = getColor(R.color.surface)
        Theme.colorPrimary = getColor(R.color.primary)
        Theme.colorOnPrimary = getColor(R.color.on_primary)
        Theme.colorText = getColor(R.color.text)
        Theme.colorTextSecondary = getColor(R.color.text_secondary)
        Theme.colorOutline = getColor(R.color.outline)

        // Before any screen or the logging service reads them.
        thresholds.load(getSharedPreferences(PREFS_NAME, Context.MODE_PRIVATE))

        // No-op on boards without a network link (FEATURE_WIFI / FEATURE_ETHERNET probe inside).
        networkManager.start(this)

        startActivity(Intent(this, HomeActivity::class.java))
    }
}
