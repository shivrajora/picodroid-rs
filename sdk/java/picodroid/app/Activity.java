package picodroid.app;

import picodroid.content.Context;
import picodroid.content.Intent;
import picodroid.content.pm.PackageManager;
import picodroid.graphics.Display;
import picodroid.view.View;

public class Activity extends Context {
  /** Called once when the Activity is first created. Build the UI tree here. */
  public void onCreate() {
    // Subclass overrides
  }

  /**
   * Called after onCreate, and again whenever this Activity returns to the foreground after another
   * Activity finished above it. The content view installed via setContentView is preserved across
   * pause: a UI built in onCreate stays alive while this Activity is paused under another, and is
   * restored automatically on resume. Rebuilding from onResume is still supported (the new root
   * replaces the saved one).
   */
  public void onStart() {
    // Subclass overrides
  }

  /** Called immediately after onStart; the Activity is now interactive. */
  public void onResume() {
    // Subclass overrides
  }

  /** Called when another Activity is being launched on top of this one. */
  public void onPause() {
    // Subclass overrides
  }

  /** Called after onPause, once the new top Activity is fully resumed. */
  public void onStop() {
    // Subclass overrides
  }

  /** Called right before this Activity is destroyed (after finish() pops it). */
  public void onDestroy() {
    // Subclass overrides
  }

  /**
   * Default BACK-key handler: finishes this Activity. Override and *don't* call super to suppress
   * (e.g. show a confirm dialog instead).
   */
  public void onBackPressed() {
    finish();
  }

  /**
   * Pop this Activity off the stack. Triggers onPause → onStop → onDestroy. If this is the last
   * Activity in the stack, the app exits.
   */
  public native void finish();

  /**
   * Launch the Activity named by {@code intent}'s target class on top of this one. The new Activity
   * goes through onCreate → onStart → onResume; this Activity goes through onPause → onStop. The
   * two are interleaved in the Android-canonical order (this.onPause first, this.onStop after the
   * new top fully resumes). The framework instantiates the target via its no-arg constructor;
   * Intent extras are not yet exposed via {@code getIntent()}.
   */
  public native void startActivity(Intent intent);

  public PackageManager getPackageManager() {
    return PackageManager.getInstance();
  }

  public void setContentView(View root) {
    Display.getInstance().setContentView(root);
  }

  public Display getDisplay() {
    return Display.getInstance();
  }
}
