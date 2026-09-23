// SPDX-License-Identifier: GPL-3.0-only
package claudeusage.data;

import claudeusage.R;

/** Why the display does, or does not, have fresh numbers. */
public enum LinkState {
  /** Boot: the radio has not joined yet. */
  JOINING(R.string.link_joining, R.string.advice_joining),
  OK(R.string.link_ok, 0),
  /** The network was up and went away. */
  NO_WIFI(R.string.link_no_wifi, R.string.advice_no_wifi),
  /** Nothing answers at the bridge's address: the PC is off, asleep, or the address is wrong. */
  PC_OFF(R.string.link_pc_off, R.string.advice_pc_off),
  /** The PC answers and refuses the connection: the bridge is not running. */
  BRIDGE_DOWN(R.string.link_bridge_down, R.string.advice_bridge_down),
  /** The bridge accepted the connection and then said nothing. */
  NO_REPLY(R.string.link_no_reply, R.string.advice_no_reply),
  /**
   * The bridge is fine but cannot get limits from Anthropic; the snapshot's {@code err} says why.
   */
  UPSTREAM(R.string.link_claude_error, R.string.advice_upstream),
  /** Non-200, oversize or unparseable reply. */
  BAD_DATA(R.string.link_bridge_error, R.string.advice_bad_data);

  private final int shortText;
  private final int advice;

  LinkState(int shortText, int advice) {
    this.shortText = shortText;
    this.advice = advice;
  }

  /** At most 14 characters: it shares the footer with the time since the last sync. */
  public int shortText(String err) {
    if (this == UPSTREAM) {
      if ("auth".equals(err)) {
        return R.string.link_login_expired;
      }
      if ("rate".equals(err)) {
        return R.string.link_rate_limited;
      }
      if ("creds".equals(err)) {
        return R.string.link_no_credentials;
      }
    }
    return shortText;
  }

  /** One line of what to do about it, for the status screen; 0 when there is nothing to say. */
  public int advice(String err) {
    if (this == UPSTREAM) {
      if ("auth".equals(err) || "creds".equals(err)) {
        return R.string.advice_sign_in;
      }
      if ("rate".equals(err)) {
        return R.string.advice_rate_limited;
      }
    }
    return advice;
  }
}
