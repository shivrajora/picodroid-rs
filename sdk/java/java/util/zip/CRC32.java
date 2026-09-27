// SPDX-License-Identifier: GPL-3.0-only
package java.util.zip;

/**
 * The JDK's {@code java.util.zip.CRC32}: the IEEE 802.3 polynomial, reflected, initial value and
 * final XOR of all ones, the checksum of {@code "123456789"} being {@code 0xCBF43926}. The two
 * {@code private static native} steps are the runtime's, in Rust, as the JDK delegates its own to
 * zlib; the running value and the public API stay here, so a {@link Checksum} reference dispatches
 * through this class like any other.
 *
 * <p>{@link #getValue} is the checksum of every byte fed since construction or the last {@link
 * #reset}, as an unsigned 32-bit value in a {@code long}. Feeding one byte at a time through {@link
 * #update(int)} costs a native call per byte; hand whole arrays to {@link #update(byte[], int,
 * int)}.
 */
public class CRC32 implements Checksum {
  /** The finalised checksum so far — 0 for no bytes, as the JDK keeps it. */
  private int crc;

  public CRC32() {}

  /** Feeds the low eight bits of {@code b}. */
  @Override
  public void update(int b) {
    crc = update(crc, b);
  }

  /** Feeds {@code len} bytes of {@code b} from {@code off}. */
  @Override
  public void update(byte[] b, int off, int len) {
    if (b == null) {
      throw new NullPointerException();
    }
    if (off < 0 || len < 0 || off > b.length - len) {
      throw new ArrayIndexOutOfBoundsException();
    }
    crc = updateBytes(crc, b, off, len);
  }

  /** Feeds the whole of {@code b}. */
  public void update(byte[] b) {
    crc = updateBytes(crc, b, 0, b.length);
  }

  @Override
  public void reset() {
    crc = 0;
  }

  @Override
  public long getValue() {
    return (long) crc & 0xffffffffL;
  }

  private static native int update(int crc, int b);

  private static native int updateBytes(int crc, byte[] b, int off, int len);
}
