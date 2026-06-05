// SPDX-License-Identifier: GPL-3.0-only
package picodroid.text;

/**
 * Input-type constants mirroring {@code android.text.InputType}. Only the class is meaningful to
 * the current soft keyboard: {@link #TYPE_CLASS_NUMBER} selects the digit pad, everything else uses
 * the default text layout. Pass to {@link picodroid.widget.EditText#setInputType(int)}.
 */
public final class InputType {

  private InputType() {}

  /** Mask selecting the input-type class from a combined type value. */
  public static final int TYPE_MASK_CLASS = 0x0000000f;

  /** Plain text. */
  public static final int TYPE_CLASS_TEXT = 0x00000001;

  /** Numeric (digit-pad) input. */
  public static final int TYPE_CLASS_NUMBER = 0x00000002;
}
