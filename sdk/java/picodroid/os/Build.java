// SPDX-License-Identifier: GPL-3.0-only
package picodroid.os;

/**
 * What this firmware was built for, mirroring {@code android.os.Build}: the board ({@link #BOARD},
 * the {@code board.toml} name), the MCU ({@link #HARDWARE}) and the release ({@link
 * VERSION#RELEASE}, the framework map version the firmware was cut with — an app's PAPK must have
 * been built against a compatible one to install).
 */
public class Build {
  /** The board, e.g. {@code testbench_rp2350}. */
  public static final String BOARD = nativeBoard();

  /** The MCU, e.g. {@code rp2350}. */
  public static final String HARDWARE = nativeHardware();

  /** Version strings. */
  public static class VERSION {
    /** The framework map version, e.g. {@code 0.21.0}. */
    public static final String RELEASE = Build.nativeRelease();
  }

  private static native String nativeBoard();

  private static native String nativeHardware();

  static native String nativeRelease();
}
