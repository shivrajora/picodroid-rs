package picodroid.concurrent;

public interface Executor {
  void execute(Runnable command);
}
