// SPDX-License-Identifier: GPL-3.0-only
package claudeusage.ui;

import claudeusage.data.LinkState;
import claudeusage.data.UsageSnapshot;

/**
 * Everything the screens show, as one immutable value: the {@link UsageViewModel} publishes a new
 * one when any of it changes, and only then. What depends on the time is held at the grain the
 * screens show it at (the minute, or the second for the status screen's countdown), so a value
 * stays current for as long as it would be painted the same.
 */
final class UsageUiState {
  /** The last reply worth showing; null before the first. Never changed once published. */
  final UsageSnapshot snapshot;

  /** Whether {@link #snapshot} is current enough to present as live. */
  final boolean fresh;

  /** The link's state, an overdue fetch already counted as an unreachable PC. */
  final LinkState link;

  /** Why the bridge could not reach Anthropic; empty unless the link is {@code UPSTREAM}. */
  final String linkErr;

  /** A fetch is in flight. */
  final boolean syncing;

  /** The wall clock in whole minutes: the header clock and every countdown are minute-grained. */
  final long minute;

  /** Wall-clock time of the last good reply, 0 if there has been none. */
  final long lastGoodWallMs;

  /** Whole minutes since the last good reply, -1 if there has been none. */
  final long staleMinutes;

  /** Where the bridge is being looked for; null while the first discovery is out. */
  final String address;

  /** Seconds to the next attempt, counted only while there is nothing else to show; else 0. */
  final int retrySeconds;

  /** Session percent per sample, oldest first; -1 is a gap. */
  final int[] trend;

  UsageUiState(
      UsageSnapshot snapshot,
      boolean fresh,
      LinkState link,
      String linkErr,
      boolean syncing,
      long minute,
      long lastGoodWallMs,
      long staleMinutes,
      String address,
      int retrySeconds,
      int[] trend) {
    this.snapshot = snapshot;
    this.fresh = fresh;
    this.link = link;
    this.linkErr = linkErr;
    this.syncing = syncing;
    this.minute = minute;
    this.lastGoodWallMs = lastGoodWallMs;
    this.staleMinutes = staleMinutes;
    this.address = address;
    this.retrySeconds = retrySeconds;
    this.trend = trend;
  }

  /** Whether there are limits to show: until then the status screen covers the pages. */
  boolean hasData() {
    return snapshot != null && snapshot.hasLimits();
  }

  /** Whether a state made of these values would be painted exactly as this one is. */
  boolean shows(
      UsageSnapshot snapshot,
      boolean fresh,
      LinkState link,
      String linkErr,
      boolean syncing,
      long minute,
      long lastGoodWallMs,
      long staleMinutes,
      String address,
      int retrySeconds,
      int[] trend) {
    return snapshot == this.snapshot
        && fresh == this.fresh
        && link == this.link
        && syncing == this.syncing
        && minute == this.minute
        && lastGoodWallMs == this.lastGoodWallMs
        && staleMinutes == this.staleMinutes
        && retrySeconds == this.retrySeconds
        && trend == this.trend
        && linkErr.equals(this.linkErr)
        && (address == null ? this.address == null : address.equals(this.address));
  }
}
