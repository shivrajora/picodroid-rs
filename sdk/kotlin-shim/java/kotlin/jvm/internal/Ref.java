// SPDX-License-Identifier: GPL-3.0-only
package kotlin.jvm.internal;

/**
 * Boxes for local variables that a lambda captures <em>and</em> mutates ({@code var count = 0; val
 * inc = { count++ }}): kotlinc allocates one of these and both the enclosing method and the lambda
 * body read and write {@code element}.
 */
public final class Ref {
  private Ref() {}

  public static final class ObjectRef<T> {
    public T element;
  }

  public static final class ByteRef {
    public byte element;
  }

  public static final class ShortRef {
    public short element;
  }

  public static final class IntRef {
    public int element;
  }

  public static final class LongRef {
    public long element;
  }

  public static final class FloatRef {
    public float element;
  }

  public static final class DoubleRef {
    public double element;
  }

  public static final class CharRef {
    public char element;
  }

  public static final class BooleanRef {
    public boolean element;
  }
}
