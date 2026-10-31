// SPDX-License-Identifier: GPL-3.0-only
package picodroid.app;

import picodroid.content.Context;
import picodroid.os.Bundle;

/**
 * A Notification displayed by a foreground Service. v1 carries only a title and body; channels,
 * icons, intents-on-tap, and notification IDs > 1 of tray-stacking are out of scope.
 *
 * <pre>{@code
 * Notification n = new Notification.Builder(this, CHANNEL_ID)
 *     .setContentTitle("Sync")
 *     .setContentText("Updating…")
 *     .build();
 * }</pre>
 */
public final class Notification {
  /** Mirrors Android: the {@link #extras} key of the title. */
  public static final String EXTRA_TITLE = "android.title";

  /** Mirrors Android: the {@link #extras} key of the text. */
  public static final String EXTRA_TEXT = "android.text";

  // Slots 0 and 1: NotificationManager.notify's native reads them by index.
  String title;
  String text;

  /**
   * Mirrors Android: what the builder set, under {@link #EXTRA_TITLE} and {@link #EXTRA_TEXT}; how
   * an app reads a notification back.
   */
  public Bundle extras = new Bundle();

  Notification() {}

  /** Builder for {@link Notification} — call chain ends with {@link #build}. */
  public static final class Builder {
    private final Notification n = new Notification();

    /**
     * Mirrors Android's {@code Notification.Builder(Context, String)}. picodroid has one
     * notification slot and no channels: {@code channelId} is accepted and not used.
     */
    public Builder(Context context, String channelId) {}

    public Builder setContentTitle(String s) {
      n.title = s;
      n.extras.putString(EXTRA_TITLE, s);
      return this;
    }

    public Builder setContentText(String s) {
      n.text = s;
      n.extras.putString(EXTRA_TEXT, s);
      return this;
    }

    public Notification build() {
      return n;
    }
  }
}
