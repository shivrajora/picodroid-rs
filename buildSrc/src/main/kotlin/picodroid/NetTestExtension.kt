// SPDX-License-Identifier: GPL-3.0-only
package picodroid

import org.gradle.api.provider.Property

/**
 * `picodroidNetTest { enabled = true }` — opt a networking example into the
 * generated [NetTestConfig][GenerateNetTestConfigTask] source. Off by
 * default so ordinary apps don't carry a dead class in their papk.
 */
abstract class NetTestExtension {
    abstract val enabled: Property<Boolean>
}
