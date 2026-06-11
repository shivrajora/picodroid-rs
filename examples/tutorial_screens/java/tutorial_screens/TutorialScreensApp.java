// SPDX-License-Identifier: GPL-3.0-only
package tutorial_screens;

import picodroid.app.Application;
import picodroid.content.Intent;

/**
 * App entry point. The framework instantiates the Application named in PicodroidManifest.xml and
 * calls onCreate once at boot; launching the first Activity here seeds the back stack with its root
 * screen.
 */
public class TutorialScreensApp extends Application {
  @Override
  public void onCreate() {
    startActivity(new Intent(HomeActivity.class));
  }
}
