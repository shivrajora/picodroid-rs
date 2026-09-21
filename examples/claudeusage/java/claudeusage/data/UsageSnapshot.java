// SPDX-License-Identifier: GPL-3.0-only
package claudeusage.data;

/** One reply from the bridge. Written once by the fetch, then only read. */
public final class UsageSnapshot {
  public static final int MAX_MODELS = 3;
  public static final int DAYS = 7;

  /** Bridge wall clock, epoch seconds, and its UTC offset: the device has no RTC of its own. */
  public long bridgeEpochS;

  public int tzMinutes;

  /** False when the bridge is up but could not reach Anthropic; {@link #err} says why. */
  public boolean ok;

  public String err = "";

  /** Seconds since the bridge last got limits from upstream, -1 if it never has. */
  public int ageS = -1;

  public String plan = "";

  /** Percent of the limit used, -1 when the bridge did not report the window. */
  public int sessionPct = -1;

  /** Epoch seconds at which the window resets, 0 if unknown. */
  public long sessionReset;

  public int weeklyPct = -1;
  public long weeklyReset;

  public int modelCount;
  public final String[] modelName = new String[MAX_MODELS];
  public final int[] modelPct = new int[MAX_MODELS];
  public final long[] modelReset = new long[MAX_MODELS];

  /** Session percent gained per hour, 0 when idle. */
  public int ratePerHour;

  /** Minutes until the session reaches 100 % at {@link #ratePerHour}, -1 when not applicable. */
  public int etaMinutes = -1;

  /** Today's tokens in thousands, estimated cost in cents (-1: no price known), messages. */
  public int todayTokensK;

  public int todayCents;
  public int todayMessages;

  public boolean hasHistory;

  /** Thousands of tokens per day, oldest first; the last entry is today. */
  public final int[] dayTokensK = new int[DAYS];

  public String dayLetters = "";

  public int mixCount;
  public final String[] mixName = new String[MAX_MODELS];
  public final int[] mixPct = new int[MAX_MODELS];

  public boolean hasLimits() {
    return sessionPct >= 0 || weeklyPct >= 0;
  }
}
