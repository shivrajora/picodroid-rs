// SPDX-License-Identifier: GPL-3.0-only
package picodroid.os;

/**
 * The interface of the object returned by {@link picodroid.app.Service#onBind}. Apps extend {@link
 * Binder} to expose a typed handle to clients (the LocalBinder pattern):
 *
 * <pre>{@code
 * public class LocalBinder extends Binder {
 *   public MyService getService() {
 *     return MyService.this;
 *   }
 * }
 * }</pre>
 *
 * Picodroid is single-process, so there is no AIDL / Messenger / true Binder IPC — {@code IBinder}
 * is just a marker that a Service exposes some object for clients to read.
 */
public interface IBinder {}
