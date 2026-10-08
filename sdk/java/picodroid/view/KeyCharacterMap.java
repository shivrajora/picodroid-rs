// SPDX-License-Identifier: GPL-3.0-only
package picodroid.view;

/**
 * Mirrors the one question apps ask {@code android.view.KeyCharacterMap}: {@link #deviceHasKey
 * whether the device has a key}. The answer covers the board's buttons as {@code board.toml} maps
 * them to key codes, and the two system keys every board has one way or another: BACK and HOME,
 * which a board without a key for them gets from the on-screen navigation control (a tap and a
 * hold) or from holding BACK (HOME). There is no character map: no board has text keys, and the
 * on-screen keyboard types.
 *
 * <p>Use it to decide what to show, not whether to handle a key: an app that handles {@link
 * KeyEvent#KEYCODE_DPAD_UP} works wherever the key exists, and a hint such as "A: up" belongs only
 * on a board where {@code deviceHasKey(KEYCODE_DPAD_UP)} is true.
 */
public final class KeyCharacterMap {
  private KeyCharacterMap() {}

  /** Mirrors Android: whether this device can produce {@code keyCode}. */
  public static boolean deviceHasKey(int keyCode) {
    return nativeDeviceHasKey(keyCode);
  }

  /** Mirrors Android: {@link #deviceHasKey} for each code, in order. */
  public static boolean[] deviceHasKeys(int[] keyCodes) {
    boolean[] out = new boolean[keyCodes.length];
    for (int i = 0; i < keyCodes.length; i++) {
      out[i] = deviceHasKey(keyCodes[i]);
    }
    return out;
  }

  private static native boolean nativeDeviceHasKey(int keyCode);
}
