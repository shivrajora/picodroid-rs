// SPDX-License-Identifier: GPL-3.0-only
package clinitdemo;

/** A class whose static initializer always throws — exercises ExceptionInInitializerError. */
public class Doomed {
  public static int VALUE;

  static {
    if (alwaysTrue()) {
      throw new RuntimeException("clinit boom");
    }
    VALUE = 1;
  }

  /** Defeats javac's constant-folding so the throw survives compilation. */
  static boolean alwaysTrue() {
    return true;
  }
}
