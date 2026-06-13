// SPDX-License-Identifier: GPL-3.0-only
package picodroid.text;

/**
 * Mirrors {@code android.text.TextWatcher}'s three callbacks. Picodroid divergences (see the
 * compatibility matrix):
 *
 * <ul>
 *   <li>Parameters are {@code String} — picodroid has no {@code CharSequence}/{@code Editable}.
 *   <li><b>Only {@link #afterTextChanged} fires.</b> {@link #beforeTextChanged} and {@link
 *       #onTextChanged} are declared for source compatibility with ported Android code but are
 *       never invoked: computing their {@code start}/{@code count} deltas needs a before/after diff
 *       the native text pipeline does not keep.
 * </ul>
 *
 * <p>Register via {@link picodroid.widget.EditText#addTextChangedListener}. As on Android, {@code
 * setText(...)} also fires the watcher.
 */
public interface TextWatcher {
  /** Declared for source compatibility — never invoked on picodroid. */
  default void beforeTextChanged(String s, int start, int count, int after) {}

  /** Declared for source compatibility — never invoked on picodroid. */
  default void onTextChanged(String s, int start, int before, int count) {}

  /** Fired after the field's content changed; {@code s} is the full new text. */
  void afterTextChanged(String s);
}
