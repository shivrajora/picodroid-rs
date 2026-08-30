// SPDX-License-Identifier: GPL-3.0-only
package injectdemokt

/**
 * Cross-file state for the demo, as top-level declarations: one `DemoStateKt` facade class instead
 * of a `companion object` per owner (every companion is a parsed class on pico-jvm). `const val`s
 * are inlined at their use sites; the `@JvmField` vars are plain static fields with no accessors.
 */
const val TAG = "InjectDemoKt"

/** Kept so HomeActivity can prove its own Greeter is a fresh (unscoped) instance. */
@JvmField var appGreeter: Greeter? = null

/** Clock's construction counter; `@Singleton` means it must stay at 1. */
@JvmField var clocksCreated = 0
