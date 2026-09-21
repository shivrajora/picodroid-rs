// SPDX-License-Identifier: GPL-3.0-only
package claudeusage.data;

/** Why the display does, or does not, have fresh numbers. */
public final class LinkState {
  /** Boot: the radio has not joined yet. */
  public static final int JOINING = 0;

  public static final int OK = 1;

  /** The network was up and went away. */
  public static final int NO_WIFI = 2;

  /** Nothing answers at the bridge's address: the PC is off, asleep, or the address is wrong. */
  public static final int PC_OFF = 3;

  /** The PC answers and refuses the connection: the bridge is not running. */
  public static final int BRIDGE_DOWN = 4;

  /** The bridge accepted the connection and then said nothing. */
  public static final int NO_REPLY = 5;

  /**
   * The bridge is fine but cannot get limits from Anthropic; the snapshot's {@code err} says why.
   */
  public static final int UPSTREAM = 6;

  /** Non-200, oversize or unparseable reply. */
  public static final int BAD_DATA = 7;

  private static final String[] NAMES = {
    "JOINING", "OK", "NO_WIFI", "PC_OFF", "BRIDGE_DOWN", "NO_REPLY", "UPSTREAM", "BAD_DATA"
  };

  private LinkState() {}

  public static String name(int state) {
    return NAMES[state];
  }

  /** At most 14 characters: it shares the footer with the time since the last sync. */
  public static String shortText(int state, String err) {
    switch (state) {
      case JOINING:
        return "Joining WiFi";
      case OK:
        return "Connected";
      case NO_WIFI:
        return "WiFi down";
      case PC_OFF:
        return "PC offline";
      case BRIDGE_DOWN:
        return "Bridge down";
      case NO_REPLY:
        return "No reply";
      case UPSTREAM:
        if ("auth".equals(err)) {
          return "Login expired";
        }
        if ("rate".equals(err)) {
          return "Rate limited";
        }
        if ("creds".equals(err)) {
          return "No credentials";
        }
        return "Claude error";
      default:
        return "Bridge error";
    }
  }

  /** One line of what to do about it, for the full-screen status page. */
  public static String advice(int state, String err) {
    switch (state) {
      case JOINING:
        return "Waiting for the network";
      case NO_WIFI:
        return "Check the access point";
      case PC_OFF:
        return "Is the PC on and awake?";
      case BRIDGE_DOWN:
        return "Start claude_usage_bridge.py";
      case NO_REPLY:
        return "The bridge is not answering";
      case UPSTREAM:
        if ("auth".equals(err) || "creds".equals(err)) {
          return "Run claude on the PC to sign in";
        }
        if ("rate".equals(err)) {
          return "Anthropic is rate limiting";
        }
        return "The bridge cannot reach Claude";
      case BAD_DATA:
        return "Unexpected reply from the bridge";
      default:
        return "";
    }
  }
}
