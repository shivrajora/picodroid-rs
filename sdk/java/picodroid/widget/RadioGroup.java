// SPDX-License-Identifier: GPL-3.0-only
package picodroid.widget;

import picodroid.content.Context;
import picodroid.view.View;

/**
 * Mirrors {@code android.widget.RadioGroup}: a vertical {@link LinearLayout} that enforces
 * single-selection across the {@link RadioButton}s added to it.
 *
 * <p>As on Android, the group wires its own internal checked-change listener onto each RadioButton
 * it tracks — call {@link #setOnCheckedChangeListener} on the <em>group</em>, not on individual
 * buttons. Buttons added without an explicit id get one auto-assigned so {@link
 * #getCheckedRadioButtonId()} stays meaningful.
 */
public class RadioGroup extends LinearLayout {
  /** Auto-id pool for buttons added without {@code setId} — high range to dodge app ids. */
  private static int nextAutoId = 0x01000000;

  private RadioButton[] buttons = new RadioButton[4];
  private int buttonCount;
  private int checkedId = View.NO_ID;
  private OnCheckedChangeListener onCheckedChangeListener;

  public RadioGroup() {
    super();
  }

  public RadioGroup(Context ctx) {
    super();
  }

  @Override
  public void addView(View child) {
    super.addView(child);
    if (child instanceof RadioButton) {
      track((RadioButton) child);
    }
  }

  private void track(RadioButton button) {
    if (button.getId() == View.NO_ID) {
      button.setId(nextAutoId++);
    }
    if (buttonCount == buttons.length) {
      RadioButton[] bigger = new RadioButton[buttons.length * 2];
      System.arraycopy(buttons, 0, bigger, 0, buttonCount);
      buttons = bigger;
    }
    buttons[buttonCount++] = button;
    button.setOnCheckedChangeListener(
        (buttonView, isChecked) -> {
          if (isChecked) {
            // Uncheck the previous selection without re-firing (setChecked
            // flips LVGL state silently), then record + notify once.
            if (checkedId != View.NO_ID && checkedId != buttonView.getId()) {
              RadioButton prev = findButton(checkedId);
              if (prev != null) {
                prev.setChecked(false);
              }
            }
            setCheckedId(buttonView.getId());
          } else if (buttonView.getId() == checkedId) {
            // Radios don't un-check on re-tap (LVGL checkboxes toggle;
            // Android radios latch) — restore the checked state silently.
            ((RadioButton) buttonView).setChecked(true);
          }
        });
    if (button.isChecked()) {
      setCheckedId(button.getId());
    }
  }

  private RadioButton findButton(int id) {
    for (int i = 0; i < buttonCount; i++) {
      if (buttons[i].getId() == id) {
        return buttons[i];
      }
    }
    return null;
  }

  private void setCheckedId(int id) {
    checkedId = id;
    if (onCheckedChangeListener != null) {
      onCheckedChangeListener.onCheckedChanged(this, id);
    }
  }

  /** Mirrors {@code RadioGroup#check(int)}: select {@code id}, unchecking the previous button. */
  public void check(int id) {
    if (id == checkedId) {
      return;
    }
    RadioButton prev = findButton(checkedId);
    if (prev != null) {
      prev.setChecked(false);
    }
    RadioButton next = findButton(id);
    if (next != null) {
      next.setChecked(true);
      setCheckedId(id);
    }
  }

  /** Mirrors {@code RadioGroup#clearCheck()}: uncheck everything, listener fires with NO_ID. */
  public void clearCheck() {
    RadioButton prev = findButton(checkedId);
    if (prev != null) {
      prev.setChecked(false);
    }
    setCheckedId(View.NO_ID);
  }

  /** Mirrors {@code RadioGroup#getCheckedRadioButtonId()}; {@link View#NO_ID} when none. */
  public int getCheckedRadioButtonId() {
    return checkedId;
  }

  public void setOnCheckedChangeListener(OnCheckedChangeListener listener) {
    this.onCheckedChangeListener = listener;
  }

  /** Mirrors {@code android.widget.RadioGroup.OnCheckedChangeListener}. */
  public interface OnCheckedChangeListener {
    void onCheckedChanged(RadioGroup group, int checkedId);
  }
}
