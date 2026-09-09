// SPDX-License-Identifier: GPL-3.0-only
package picodroid.os;

/**
 * What this firmware was built for, mirroring {@code android.os.Build}: the board ({@link #BOARD},
 * the {@code board.toml} name), the MCU ({@link #HARDWARE}) and the release ({@link
 * VERSION#RELEASE}, the firmware's version — the one a shrink map is cut for, so on a shrunk image
 * it is also the framework map version an app's PAPK must be compatible with to install).
 */
public class Build {
  /** The board, e.g. {@code testbench_rp2350}. */
  public static final String BOARD = nativeBoard();

  /** The MCU, e.g. {@code rp2350}. */
  public static final String HARDWARE = nativeHardware();

  /** Version strings. */
  public static class VERSION {
    /** The firmware release, e.g. {@code 0.23.0}. */
    public static final String RELEASE = Build.nativeRelease();
  }

  private static native String nativeBoard();

  private static native String nativeHardware();

  static native String nativeRelease();
}
