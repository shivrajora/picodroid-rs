// SPDX-License-Identifier: GPL-3.0-only
package claudeusage.data;

/**
 * What the app knows, as one immutable value: the last reply from the bridge, where the link to it
 * stands and the last hour's trend. {@link UsageService} publishes a new one whenever a fetch
 * starts or ends, the network comes or goes, the bridge is found somewhere else or a trend sample
 * is taken; nothing in one ever changes. What depends on the time of asking (an overdue fetch, how
 * long since the last good reply) is answered for a given {@code SystemClock.elapsedRealtime()}.
 */
public final class UsageData {
  /**
   * A fetch is bounded by the connect and read timeouts, 8 s together. One still running after this
   * long is wedged, and the UI reports the PC as unreachable rather than "contacting" forever.
   */
  private static final int SYNC_OVERDUE_MS = 12_000;

  /** Two and a half polls without a good reply and the numbers on screen are no longer "live". */
  private static final int STALE_AFTER_MS = 150_000;

  /** Limits the bridge itself has not refreshed for this long count as stale too. */
  private static final int UPSTREAM_STALE_S = 900;

  /** Before anything is known: no reply, and the radio has not joined yet. */
  static final UsageData NONE =
      new UsageData(null, new int[0], LinkState.JOINING, "", null, -1, -1, 0, 0);

  /**
   * The last reply that carried limits, else the last reply of any kind; null before the first.
   * Written once by the fetch, then only read.
   */
  public final UsageSnapshot snapshot;

  /**
   * Session percent per sample, oldest first, one sample per {@link UsageService#TREND_PERIOD_MS}
   * and at most {@link UsageService#TREND_SLOTS}; -1 is a gap (no live data for that slot).
   */
  public final int[] trend;

  private final LinkState state;

  /** Why the bridge could not reach Anthropic, for {@link LinkState#UPSTREAM}; else empty. */
  public final String err;

  /**
   * The address being tried, {@code host:port}; null while the first discovery broadcast is still
   * out. Shown on the status screen so a wrong address is obvious.
   */
  public final String address;

  /** When the fetch in flight started, -1 when there is none. */
  private final long syncStartedElapsedMs;

  /** When the last good reply arrived, -1 if there has been none. */
  private final long lastGoodElapsedMs;

  /** Wall-clock time of the last good reply, 0 if there has been none. */
  public final long lastGoodWallMs;

  private final long nextAttemptElapsedMs;

  UsageData(
      UsageSnapshot snapshot,
      int[] trend,
      LinkState state,
      String err,
      String address,
      long syncStartedElapsedMs,
      long lastGoodElapsedMs,
      long lastGoodWallMs,
      long nextAttemptElapsedMs) {
    this.snapshot = snapshot;
    this.trend = trend;
    this.state = state;
    this.err = err;
    this.address = address;
    this.syncStartedElapsedMs = syncStartedElapsedMs;
    this.lastGoodElapsedMs = lastGoodElapsedMs;
    this.lastGoodWallMs = lastGoodWallMs;
    this.nextAttemptElapsedMs = nextAttemptElapsedMs;
  }

  /** The link state to present: an overdue fetch counts as an unreachable PC. */
  public LinkState stateAt(long elapsedMs) {
    if (syncStartedElapsedMs >= 0 && elapsedMs - syncStartedElapsedMs > SYNC_OVERDUE_MS) {
      return LinkState.PC_OFF;
    }
    return state;
  }

  /** True while a fetch is in flight and not yet overdue. */
  public boolean syncingAt(long elapsedMs) {
    return syncStartedElapsedMs >= 0 && elapsedMs - syncStartedElapsedMs <= SYNC_OVERDUE_MS;
  }

  /** Milliseconds since the last good reply, -1 if there has been none. */
  public long sinceLastGoodMs(long elapsedMs) {
    return lastGoodElapsedMs < 0 ? -1 : elapsedMs - lastGoodElapsedMs;
  }

  public int secondsToNextAttempt(long elapsedMs) {
    long ms = nextAttemptElapsedMs - elapsedMs;
    return ms <= 0 ? 0 : (int) ((ms + 999) / 1000);
  }

  /** True when {@link #snapshot} is current enough to present as live. */
  public boolean isFresh(long elapsedMs) {
    if (snapshot == null || !snapshot.hasLimits() || lastGoodElapsedMs < 0) {
      return false;
    }
    if (elapsedMs - lastGoodElapsedMs >= STALE_AFTER_MS) {
      return false;
    }
    return snapshot.ageS >= 0 && snapshot.ageS < UPSTREAM_STALE_S;
  }
}
