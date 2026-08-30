// SPDX-License-Identifier: GPL-3.0-only
package picoenvmonkt.hardware

import javax.inject.Inject
import javax.inject.Singleton
import picodroid.pio.PeripheralManager
import picodroid.pio.Pwm
import picodroid.util.Log

private const val TAG = "RgbLedKt"
private const val PWM_FREQ_HZ = 1000.0

/**
 * Pimoroni Enviro+ Pack RGB LED (common-anode active-low) on R=GP6, G=GP7, B=GP10. Pre-allocated at
 * the app scope (one LED on the board) and driven via PWM at ~1 kHz.
 */
@Singleton
class RgbLed @Inject constructor() {
    private val red: Pwm
    private val green: Pwm
    private val blue: Pwm

    init {
        val pm = PeripheralManager.getInstance()
        red = pm.openPwm("GP6")
        green = pm.openPwm("GP7")
        blue = pm.openPwm("GP10")
        setupChannel(red)
        setupChannel(green)
        setupChannel(blue)
        Log.i(TAG, "RGB LED ready on GP6/GP7/GP10")
        setColor(0, 0, 0)
    }

    private fun setupChannel(ch: Pwm) {
        ch.setPwmFrequencyHz(PWM_FREQ_HZ)
        ch.setEnabled(true)
    }

    /** Each component in `[0..255]`. Common-anode → 0% duty = full on. */
    fun setColor(r: Int, g: Int, b: Int) {
        red.setPwmDutyCycle(toDuty(r))
        green.setPwmDutyCycle(toDuty(g))
        blue.setPwmDutyCycle(toDuty(b))
    }

    fun off() {
        setColor(0, 0, 0)
    }

    private fun toDuty(v: Int): Double {
        val clamped = if (v < 0) 0 else if (v > 255) 255 else v
        return 100.0 * (1.0 - clamped / 255.0)
    }
}
