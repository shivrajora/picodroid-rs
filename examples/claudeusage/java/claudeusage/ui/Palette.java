// SPDX-License-Identifier: GPL-3.0-only
package claudeusage.ui;

/** A warm dark theme around Claude's clay orange. */
public final class Palette {
  public static final int BACKGROUND = 0xFF0F0E0D;

  /** Keep in step with CARD in tools/gen_digits.py: the numeral sprites are drawn onto it. */
  public static final int CARD = 0xFF1C1A18;

  public static final int TRACK = 0xFF2E2A26;
  public static final int TEXT = 0xFFF0EEE6;
  public static final int MUTED = 0xFF8A857C;
  public static final int FAINT = 0xFF57524B;
  public static final int CLAY = 0xFFD97757;
  public static final int CLAY_DEEP = 0xFF9C4F36;

  /** History bars for days that are over. */
  public static final int BAR_PAST = 0xFF5C4A40;

  public static final int GOOD = 0xFF7BC47F;
  public static final int GOOD_DEEP = 0xFF4E8A55;
  public static final int WARN = 0xFFE8B04B;
  public static final int WARN_DEEP = 0xFFB07A24;
  public static final int BAD = 0xFFE5534B;
  public static final int BAD_DEEP = 0xFFA5332D;

  /** Below this a limit is comfortable; above {@link #BAD_FROM} it is nearly gone. */
  public static final int WARN_FROM = 60;

  public static final int BAD_FROM = 85;

  private Palette() {}

  public static int severity(int pct) {
    return pct >= BAD_FROM ? BAD : (pct >= WARN_FROM ? WARN : GOOD);
  }

  public static int severityDeep(int pct) {
    return pct >= BAD_FROM ? BAD_DEEP : (pct >= WARN_FROM ? WARN_DEEP : GOOD_DEEP);
  }
}
