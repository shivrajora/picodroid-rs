// SPDX-License-Identifier: GPL-3.0-only
package picodroid.app;

import picodroid.content.Context;
import picodroid.content.Intent;
import picodroid.graphics.Display;
import picodroid.os.Bundle;
import picodroid.view.LayoutInflater;
import picodroid.view.View;

public class Activity extends Context {
  /** Standard activity result: the operation succeeded. Matches Android's value. */
  public static final int RESULT_OK = -1;

  /** Standard activity result: the operation was canceled (the default). Matches Android. */
  public static final int RESULT_CANCELED = 0;

  /** First user-definable result code. Matches Android. */
  public static final int RESULT_FIRST_USER = 1;

  /** The root last passed to {@link #setContentView}, for {@link #findViewById}. */
  private View mContentView;

  /**
   * Called when the Activity is starting. Build the UI tree here. Mirrors {@code
   * android.app.Activity#onCreate(Bundle)}.
   *
   * @param savedInstanceState the Bundle this Activity's previous instance filled in {@link
   *     #onSaveInstanceState} when the framework is re-creating it (see {@link #recreate}), else
   *     {@code null} — a fresh launch.
   */
  protected void onCreate(Bundle savedInstanceState) {
    // Subclass overrides
  }

  /**
   * Called before the framework destroys this Activity in order to re-create it (see {@link
   * #recreate}), after {@link #onStop}. Put whatever the next instance needs into {@code outState};
   * it comes back as the argument of {@link #onCreate(Bundle)} and {@link #onRestoreInstanceState}.
   * Not called for an Activity that is finishing: nothing will be re-created, so there is nothing
   * to save for. Mirrors {@code android.app.Activity#onSaveInstanceState(Bundle)}, except that the
   * default saves nothing — there are no view ids to key a view hierarchy's state by.
   */
  protected void onSaveInstanceState(Bundle outState) {
    // Subclass overrides
  }

  /**
   * Called after {@link #onStart} on a re-created Activity, with the Bundle its previous instance
   * saved; never called on a fresh launch, so {@code savedInstanceState} is never {@code null}.
   * Restoring here instead of in {@link #onCreate(Bundle)} is a matter of taste. Mirrors {@code
   * android.app.Activity#onRestoreInstanceState(Bundle)}.
   */
  protected void onRestoreInstanceState(Bundle savedInstanceState) {
    // Subclass overrides
  }

  /**
   * Destroy this Activity and start a new instance of it in its place, carrying the saved instance
   * state over: this instance gets onPause → onStop → onSaveInstanceState → onDestroy and its
   * content view is freed, then the new one gets onCreate(saved) → onStart →
   * onRestoreInstanceState(saved) → onResume. The Intent, and a pending {@code
   * startActivityForResult} launch, carry over. Takes effect after the current callback returns.
   * Mirrors {@code android.app.Activity#recreate()}; only the foreground Activity can be
   * re-created.
   */
  public native void recreate();

  // The framework enters the three Bundle callbacks through these, never by name on the app's
  // class: a native-side lookup is flat (it sees only methods the named class itself declares, so
  // it misses an override on an app's base Activity) and blind to descriptors, where an
  // invokevirtual from here walks the hierarchy and matches the signature.

  final void performCreate(Bundle savedInstanceState) {
    onCreate(savedInstanceState);
  }

  final void performSaveInstanceState(Bundle outState) {
    onSaveInstanceState(outState);
  }

  final void performRestoreInstanceState(Bundle savedInstanceState) {
    onRestoreInstanceState(savedInstanceState);
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

  public void setContentView(View root) {
    mContentView = root;
    Display.getInstance().setContentView(root);
  }

  /** Mirrors Android: inflates {@code R.layout.*} and makes it this Activity's content. */
  public void setContentView(int layoutResID) {
    setContentView(getLayoutInflater().inflate(layoutResID, null));
  }

  /** Mirrors Android: an inflater that creates views with this Activity as their context. */
  public LayoutInflater getLayoutInflater() {
    return LayoutInflater.from(this);
  }

  /**
   * Mirrors Android: the view with {@code android:id="@+id/…"} (or {@link View#setId}) {@code id}
   * in this Activity's content, or {@code null}.
   */
  @SuppressWarnings("TypeParameterUnusedInFormals") // Android's signature, since API 26
  public <T extends View> T findViewById(int id) {
    return mContentView == null ? null : mContentView.<T>findViewById(id);
  }

  public Display getDisplay() {
    return Display.getInstance();
  }
}
