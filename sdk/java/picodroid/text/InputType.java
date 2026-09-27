// SPDX-License-Identifier: GPL-3.0-only
package picodroid.text;

/**
 * Input-type constants mirroring {@code android.text.InputType}, with Android's exact bit values.
 * Pass to {@link picodroid.widget.EditText#setInputType(int)}.
 *
 * <p>The input-type <em>class</em> picks the soft keyboard's layout: {@link #TYPE_CLASS_NUMBER}
 * (and {@link #TYPE_CLASS_PHONE}, which shares the digit pad) selects the numeric layout;
 * everything else uses the default text layout. Of the variations, the password ones mask the
 * field; the others and the flag bits are accepted so ported Android code compiles unchanged, but
 * do not yet specialize the keyboard.
 */
public final class InputType {

  private InputType() {}

  /** Mask selecting the input-type class from a combined type value. */
  public static final int TYPE_MASK_CLASS = 0x0000000f;

  /** Mask selecting the variation bits from a combined type value. */
  public static final int TYPE_MASK_VARIATION = 0x00000ff0;

  /** Mask selecting the flag bits from a combined type value. */
  public static final int TYPE_MASK_FLAGS = 0x00fff000;

  /** Plain text. */
  public static final int TYPE_CLASS_TEXT = 0x00000001;

  /** Numeric (digit-pad) input. */
  public static final int TYPE_CLASS_NUMBER = 0x00000002;

  /** Phone number. Uses the digit pad — picodroid does not have a dedicated phone layout. */
  public static final int TYPE_CLASS_PHONE = 0x00000003;

  /** Date/time. Falls back to the text layout in v1. */
  public static final int TYPE_CLASS_DATETIME = 0x00000004;

  /** Text variation: URI. Accepted; keyboard layout does not yet specialize. */
  public static final int TYPE_TEXT_VARIATION_URI = 0x00000010;

  /** Text variation: email address. Accepted; keyboard layout does not yet specialize. */
  public static final int TYPE_TEXT_VARIATION_EMAIL_ADDRESS = 0x00000020;

  /** Text variation: password. The field shows bullets; {@code getText()} reads the real text. */
  public static final int TYPE_TEXT_VARIATION_PASSWORD = 0x00000080;

  /** Text variation: a password shown in clear, as on Android. */
  public static final int TYPE_TEXT_VARIATION_VISIBLE_PASSWORD = 0x00000090;

  /** Text variation: a web form's password. Masked like {@link #TYPE_TEXT_VARIATION_PASSWORD}. */
  public static final int TYPE_TEXT_VARIATION_WEB_PASSWORD = 0x000000e0;

  /** Number variation: a numeric password (a PIN). Masked. */
  public static final int TYPE_NUMBER_VARIATION_PASSWORD = 0x00000010;

  /** Number flag: allow a sign character. Accepted; not yet enforced. */
  public static final int TYPE_NUMBER_FLAG_SIGNED = 0x00001000;

  /** Number flag: allow a decimal point. Accepted; not yet enforced. */
  public static final int TYPE_NUMBER_FLAG_DECIMAL = 0x00002000;
}
