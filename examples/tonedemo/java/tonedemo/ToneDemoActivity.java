// SPDX-License-Identifier: GPL-3.0-only
package tonedemo;

import picodroid.app.Activity;
import picodroid.graphics.Color;
import picodroid.graphics.Display;
import picodroid.media.AudioManager;
import picodroid.media.ToneGenerator;
import picodroid.util.Log;
import picodroid.view.View;
import picodroid.widget.Button;
import picodroid.widget.LinearLayout;
import picodroid.widget.TextView;

/**
 * Tones on the board's buzzer, through {@link ToneGenerator}.
 *
 * <p>Four buttons: a single Android tone, a repeating one that has to be stopped by hand, a melody
 * through the picodroid sequence extension, and stop. A short chime plays on entry so that a
 * headless run shows the buzzer working without anyone touching the screen.
 *
 * <p>Note where the work happens. Tones advance on the UI frame tick, so the listeners below start
 * a tone and return immediately rather than sleeping between notes. Blocking this thread would
 * stall playback exactly as it stalls animation.
 */
public class ToneDemoActivity extends Activity {
  private static final String TAG = "ToneDemo";

  /** C, E, G, C — a plain major arpeggio, in Hz. */
  private static final int[] CHIME_HZ = {262, 330, 392, 523};

  private static final int[] CHIME_MS = {120, 120, 120, 320};

  private ToneGenerator tones;

  @Override
  public void onCreate() {
    // Bring LVGL up before constructing widgets, as the other UI examples do.
    Display display = getDisplay();

    tones = new ToneGenerator(AudioManager.STREAM_SYSTEM, 80);

    LinearLayout root = new LinearLayout();
    root.setOrientation(LinearLayout.VERTICAL);
    // Fill the panel rather than shrinking to the buttons: a root sized to its
    // content clips the last row on a short screen.
    root.setSize(display.getWidth(), display.getHeight());
    root.setPadding(10, 10, 10, 10);

    TextView title = new TextView();
    title.setText("Tone Demo");
    title.setTextColor(Color.WHITE);
    root.addView(title);

    root.addView(button("Beep", v -> play("TONE_PROP_BEEP", ToneGenerator.TONE_PROP_BEEP)));
    root.addView(
        button("Ringtone", v -> play("TONE_SUP_RINGTONE", ToneGenerator.TONE_SUP_RINGTONE)));
    root.addView(
        button(
            "Melody",
            v -> {
              boolean ok = tones.startToneSequence(CHIME_HZ, CHIME_MS);
              Log.i(TAG, "startToneSequence -> " + ok);
            }));
    root.addView(
        button(
            "Stop",
            v -> {
              tones.stopTone();
              Log.i(TAG, "stopped");
            }));

    setContentView(root);

    // Play on entry so a run with no input still proves the buzzer works.
    Log.i(TAG, "ready; playing the entry chime");
    Log.i(TAG, "startToneSequence -> " + tones.startToneSequence(CHIME_HZ, CHIME_MS));
  }

  private Button button(String label, View.OnClickListener onClick) {
    Button b = new Button(label);
    b.setSize(200, 44);
    b.setOnClickListener(onClick);
    return b;
  }

  private void play(String name, int toneType) {
    boolean ok = tones.startTone(toneType);
    Log.i(TAG, "startTone(" + name + ") -> " + ok);
  }

  @Override
  public void onDestroy() {
    if (tones != null) {
      tones.release();
      tones = null;
    }
  }
}
