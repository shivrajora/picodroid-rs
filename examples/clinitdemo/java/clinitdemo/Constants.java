package clinitdemo;

/** Helper class with its own static initializers, used to verify cross-class clinit chaining. */
public class Constants {
  public static int MAGIC = 7;
  public static int DERIVED;

  static {
    DERIVED = MAGIC * 3;
  }
}
