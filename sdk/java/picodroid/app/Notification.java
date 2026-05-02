// SPDX-License-Identifier: GPL-3.0-only
package picodroid.app;

/**
 * A Notification displayed by a foreground Service. v1 carries only a title and body; channels,
 * icons, intents-on-tap, and notification IDs > 1 of tray-stacking are out of scope.
 *
 * <pre>{@code
 * Notification n = new Notification.Builder()
 *     .setContentTitle("Sync")
 *     .setContentText("Updating…")
 *     .build();
 * }</pre>
 */
public final class Notification {
  String title;
  String text;

  Notification() {}

  public String getContentTitle() {
    return title;
  }

  public String getContentText() {
    return text;
  }

  /** Builder for {@link Notification} — call chain ends with {@link #build}. */
  public static final class Builder {
    private final Notification n = new Notification();

    public Builder setContentTitle(String s) {
      n.title = s;
      return this;
    }

    public Builder setContentText(String s) {
      n.text = s;
      return this;
    }

    public Notification build() {
      return n;
    }
  }
}
