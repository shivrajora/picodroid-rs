package picodroid.concurrent;

public class Thread {
  private Runnable target;

  public Thread(Runnable target) {
    this.target = target;
  }

  public native void start();
}
