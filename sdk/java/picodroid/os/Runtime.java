package picodroid.os;

public class Runtime {
  public static native long gcTimeNanos();

  public static native int gcCount();

  public static native int gcFreed();

  public static native void resetGcStats();
}
