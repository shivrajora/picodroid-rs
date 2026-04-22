package picodroid.concurrent;

final class MainExecutor implements Executor {
  @Override
  public native void execute(Runnable command);
}
