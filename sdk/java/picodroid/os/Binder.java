// SPDX-License-Identifier: GPL-3.0-only
package picodroid.os;

/**
 * Base class for the object a {@code Service} returns from {@code onBind}. Mirrors {@code
 * android.os.Binder} as far as the LocalBinder pattern uses it:
 *
 * <pre>{@code
 * public class LocalBinder extends Binder {
 *   public MyService getService() {
 *     return MyService.this;
 *   }
 * }
 * }</pre>
 *
 * Picodroid is single-process, so there are no transactions to implement: a Binder is the object
 * the client casts and calls.
 */
public class Binder implements IBinder {
  public Binder() {}
}
