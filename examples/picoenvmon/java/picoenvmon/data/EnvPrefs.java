// SPDX-License-Identifier: GPL-3.0-only
package picoenvmon.data;

import javax.inject.Inject;
import javax.inject.Singleton;
import picodroid.content.SharedPreferences;
import picoenvmon.EnvApp;

/**
 * The app's {@link SharedPreferences} file as an injectable singleton. SDK types have no
 * {@code @Inject} constructor and there is no {@code @Provides} yet, so a one-line wrapper is the
 * way to put a framework object into the graph.
 */
@Singleton
public class EnvPrefs {
  private final SharedPreferences prefs;

  @Inject
  public EnvPrefs() {
    this.prefs = SharedPreferences.open(EnvApp.PREFS_NAME);
  }

  public SharedPreferences get() {
    return prefs;
  }
}
