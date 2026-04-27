package picodroid.app;

/**
 * Marker interface for the object returned by {@link Service#onBind}. Apps subclass it to expose a
 * typed handle to clients (the LocalBinder pattern):
 *
 * <pre>{@code
 * static class LocalBinder implements IBinder {
 *   MyService service;
 * }
 * }</pre>
 *
 * Picodroid is single-process, so there is no AIDL / Messenger / true Binder IPC — {@code IBinder}
 * is just a marker that a Service exposes some object for clients to read.
 */
public interface IBinder {}
