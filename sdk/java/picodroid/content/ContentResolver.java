// SPDX-License-Identifier: GPL-3.0-only
package picodroid.content;

/**
 * Mirrors the handle side of {@code android.content.ContentResolver}: what {@link
 * Context#getContentResolver()} returns and {@link picodroid.provider.Settings.System} takes. There
 * are no content providers here, so it carries nothing; it exists so {@code
 * Settings.System.getInt(getContentResolver(), …)} reads as it does on Android.
 */
public class ContentResolver {
  static final ContentResolver INSTANCE = new ContentResolver();

  ContentResolver() {}
}
