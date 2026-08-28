// SPDX-License-Identifier: GPL-3.0-only
package survey

import picodroid.hardware.Sensor

/** Data classes: `toString`/`equals`/`hashCode`/`copy`/`componentN` codegen with Int/Float/Long/String fields. */
data class Reading(val type: Int, val value: Float, val ts: Long)

data class Named(val name: String, val n: Int)

/** Enum with constructor params, used via `entries`, `values()`, `valueOf`, and exhaustive `when`. */
enum class SensorKind(val label: String, val sdkType: Int) {
    TEMPERATURE("temp", Sensor.TYPE_AMBIENT_TEMPERATURE),
    HUMIDITY("hum", Sensor.TYPE_RELATIVE_HUMIDITY),
    PRESSURE("press", Sensor.TYPE_PRESSURE),
    LIGHT("lux", Sensor.TYPE_LIGHT);

    companion object {
        fun fromSdk(t: Int): SensorKind? = entries.firstOrNull { it.sdkType == t }

        fun count(): Int = values().size

        fun parse(s: String): SensorKind = valueOf(s)
    }
}

sealed class Sample {
    data class Ok(val r: Reading) : Sample()

    object Missing : Sample()

    class Err(val msg: String) : Sample()
}

fun describeKind(k: SensorKind): String = when (k) {
    SensorKind.TEMPERATURE -> "T ${k.label}"
    SensorKind.HUMIDITY -> "H"
    SensorKind.PRESSURE -> "P"
    SensorKind.LIGHT -> "L"
}

fun renderSample(s: Sample): String = when (s) {
    is Sample.Ok -> "ok ${s.r} copy=${s.r.copy(value = 0f)}"
    Sample.Missing -> "missing"
    is Sample.Err -> "err ${s.msg}"
}

fun sameReading(a: Reading, b: Reading): Boolean = a == b && a.hashCode() == b.hashCode()

class Threshold(val v: Float) : Comparable<Threshold> {
    override fun compareTo(other: Threshold): Int = v.compareTo(other.v)
}

interface Describable {
    fun describe(): String = "d"
}

interface Tagged {
    fun describe(): String = "t"

    fun tag(): String
}

class Both : Describable, Tagged {
    override fun describe(): String = super<Describable>.describe() + super<Tagged>.describe()

    override fun tag(): String = "both"
}

class OnlyDefault : Describable

fun useDefaults(): String = Both().describe() + Both().tag() + OnlyDefault().describe()
