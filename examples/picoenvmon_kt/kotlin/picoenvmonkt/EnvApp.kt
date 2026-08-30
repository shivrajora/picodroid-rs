// SPDX-License-Identifier: GPL-3.0-only
package picoenvmonkt

import javax.inject.Inject
import picodroid.app.Application
import picodroid.content.Intent
import picodroid.graphics.Color
import picodroid.graphics.Theme
import picoenvmonkt.net.NetworkManager
import picoenvmonkt.ui.home.HomeActivity

/**
 * App entry point. The object graph is wired by `@Inject`/`@Singleton`
 * (docs/designs/inject-annotations-2026-08.md), processed by kapt: the framework injects this
 * Application's fields before `onCreate`, and every Activity and Service below gets the same
 * treatment, so the singletons ([NetworkManager], ThresholdConfig, Formatter, LatestReadings,
 * RgbLed) are shared without a hand-written component; SDK types come from [EnvModule].
 */
class EnvApp : Application() {
    /** Owns the WiFi join, dashboard server, NTP sync and weather refresh. */
    @Inject lateinit var networkManager: NetworkManager

    override fun onCreate() {
        Theme.colorBackground = Color.argb(255, 14, 20, 24)
        Theme.colorSurface = Color.argb(255, 24, 36, 44)
        Theme.colorPrimary = Color.argb(255, 38, 166, 154)
        Theme.colorOnPrimary = Color.WHITE
        Theme.colorText = Color.argb(255, 240, 240, 240)
        Theme.colorTextSecondary = Color.argb(255, 160, 180, 188)
        Theme.colorOutline = Color.argb(255, 56, 80, 92)

        // No-op on boards without WiFi (FEATURE_WIFI probe inside).
        networkManager.start()

        startActivity(Intent(HomeActivity::class.java))
    }
}
