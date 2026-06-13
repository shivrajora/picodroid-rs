// SPDX-License-Identifier: GPL-3.0-only
package picodroid.app;

import picodroid.content.Context;
import picodroid.content.Intent;
import picodroid.content.pm.PackageManager;
import picodroid.graphics.Display;
import picodroid.view.View;

public class Activity extends Context {
  /** Standard activity result: the operation succeeded. Matches Android's value. */
  public static final int RESULT_OK = -1;

  /** Standard activity result: the operation was canceled (the default). Matches Android. */
  public static final int RESULT_CANCELED = 0;

  /** First user-definable result code. Matches Android. */
  public static final int RESULT_FIRST_USER = 1;

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

  /**
   * Called when this Activity returns to the foreground after being stopped (the Activity above it
   * finished), before {@link #onStart}. Mirrors Android's stopped → restarted edge; not called on
   * the first launch.
   */
  public void onRestart() {
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
   * new top fully resumes). The framework instantiates the target via its no-arg constructor; the
   * Intent (extras included) is retained and available to the target via {@link #getIntent()}.
   */
  public native void startActivity(Intent intent);

  /**
   * Launch an Activity expecting a result. Mirrors {@code
   * android.app.Activity#startActivityForResult}. When the launched Activity finishes, its result
   * (set via {@link #setResult}) is delivered to {@link #onActivityResult} on this Activity — after
   * {@code onRestart}'s restore but before {@code onResume}, the Android ordering.
   */
  public native void startActivityForResult(Intent intent, int requestCode);

  /**
   * Set the result this Activity reports to its launcher. Mirrors {@code
   * android.app.Activity#setResult(int)}; the default if never called is {@link #RESULT_CANCELED}.
   */
  public native void setResult(int resultCode);

  /**
   * Set the result code and an Intent of result data. Mirrors {@code
   * android.app.Activity#setResult(int, Intent)}. The Intent's extras are readable in the
   * launcher's {@link #onActivityResult}.
   */
  public native void setResult(int resultCode, Intent data);

  /**
   * Called on the launching Activity when an Activity it started for a result finishes. Mirrors
   * {@code android.app.Activity#onActivityResult}. Default no-op; override to read the result.
   * {@code data} is {@code null} unless the child called {@code setResult(int, Intent)}.
   */
  protected void onActivityResult(int requestCode, int resultCode, Intent data) {
    // Subclass overrides
  }

  /**
   * Return the Intent that launched this Activity, or {@code null} for the app's boot Activity
   * (which the framework starts without an app-visible Intent). Mirrors {@code
   * android.app.Activity#getIntent()} — read extras via {@code getIntent().getStringExtra(...)}.
   */
  public native Intent getIntent();

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
