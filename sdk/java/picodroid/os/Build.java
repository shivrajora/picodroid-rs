// SPDX-License-Identifier: GPL-3.0-only
package picodroid.os;

/**
 * What this firmware was built for, mirroring {@code android.os.Build}: the board ({@link #BOARD},
 * the {@code board.toml} name), the MCU ({@link #HARDWARE}), the release ({@link VERSION#RELEASE},
 * the firmware's version — the one a shrink map is cut for, so on a shrunk image it is also the
 * framework map version an app's PAPK must be compatible with to install) and this device's serial
 * number ({@link #getSerial()}).
 */
public class Build {
  /** Value used for a build property that is not known. */
  public static final String UNKNOWN = "unknown";

  /** The board, e.g. {@code testbench_rp2350}. */
  public static final String BOARD = nativeBoard();

  /** The MCU, e.g. {@code rp2350}. */
  public static final String HARDWARE = nativeHardware();

  /**
   * Always {@link #UNKNOWN}, as for an Android app targeting O or later, so code ported from one
   * behaves the same here.
   *
   * @deprecated Use {@link #getSerial()}.
   */
  @Deprecated public static final String SERIAL = UNKNOWN;

  /** Version strings. */
  public static class VERSION {
    /** The firmware release, e.g. {@code 0.37.0}. */
    public static final String RELEASE = Build.nativeRelease();
  }

  /**
   * The hardware serial number, e.g. {@code E6633861A37B9038}: the same value as the device's USB
   * serial number and the serial {@code pdb devices} lists. {@link #UNKNOWN} on a device that has
   * none.
   *
   * <p>Android requires {@code READ_PRIVILEGED_PHONE_STATE} and throws {@code SecurityException}
   * without it. Picodroid has no permission model yet and returns the serial to every app; any app
   * on the host can read it from the USB descriptor anyway.
   */
  public static String getSerial() {
    return nativeSerial();
  }

  private static native String nativeBoard();

  private static native String nativeHardware();

  private static native String nativeSerial();

  static native String nativeRelease();
}
