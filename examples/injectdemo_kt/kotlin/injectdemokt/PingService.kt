// SPDX-License-Identifier: GPL-3.0-only
package injectdemokt

import javax.inject.Inject
import picodroid.app.Service
import picodroid.content.Intent
import picodroid.os.IBinder
import picodroid.util.Log

/** Services are framework-owned too: fields are injected before onCreate. */
class PingService : Service() {
    @Inject lateinit var clock: Clock

    override fun onCreate() {
        Log.i(TAG, "Service clock#${clock.id}")
    }

    override fun onBind(intent: Intent): IBinder? = null
}
