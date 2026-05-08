// SPDX-License-Identifier: GPL-3.0-only
package displaydemo;

import picodroid.app.Activity;
import picodroid.graphics.Color;
import picodroid.pio.Gpio;
import picodroid.pio.PeripheralManager;
import picodroid.util.Log;
import picodroid.view.View;
import picodroid.widget.Button;
import picodroid.widget.CheckBox;
import picodroid.widget.ImageView;
import picodroid.widget.LinearLayout;
import picodroid.widget.ProgressBar;
import picodroid.widget.ScrollView;
import picodroid.widget.SeekBar;
import picodroid.widget.Spinner;
import picodroid.widget.Switch;
import picodroid.widget.TextView;
import picodroid.widget.ToggleButton;

public class DisplayDemoActivity extends Activity {
  public void onCreate() {
    getDisplay().calibrate();
    getDisplay().showFps();
    Log.i("DisplayDemo", "Display ready");

    ScrollView scroll = new ScrollView();
    scroll.setSize(320, 240);

    LinearLayout root = new LinearLayout();
    root.setOrientation(LinearLayout.VERTICAL);
    root.setSize(320, View.WRAP_CONTENT);
    root.setPadding(10, 5, 10, 5);
    root.setSpacing(4);
    scroll.addView(root);

    TextView title = new TextView();
    title.setText("Picodroid UI Demo");
    title.setTextColor(Color.WHITE);
    title.setIncludeFontPadding(false);
    root.addView(title);

    // -- Button with tap counter --
    Button btn = new Button("Tap Me!");
    btn.setSize(200, 50);
    btn.setOnClickListener(new TapHandler(title));
    root.addView(btn);

    // -- ToggleButton controlling LED --
    TextView toggleLabel = new TextView();
    toggleLabel.setText("LED: OFF");
    toggleLabel.setTextColor(Color.WHITE);
    toggleLabel.setIncludeFontPadding(false);
    root.addView(toggleLabel);

    ToggleButton toggle = new ToggleButton("ON", "OFF");
    toggle.setSize(200, 50);
    PeripheralManager manager = PeripheralManager.getInstance();
    Gpio led = manager.openGpio("GP25");
    led.setDirection(Gpio.DIRECTION_OUT_INITIALLY_LOW);
    toggle.setOnCheckedChangeListener(
        (view, checked) -> {
          if (checked) {
            toggleLabel.setText("LED: ON");
            led.setValue(true);
          } else {
            toggleLabel.setText("LED: OFF");
            led.setValue(false);
          }
        });
    root.addView(toggle);

    // -- Switch --
    TextView switchLabel = new TextView();
    switchLabel.setText("Switch: OFF");
    switchLabel.setTextColor(Color.WHITE);
    switchLabel.setIncludeFontPadding(false);
    root.addView(switchLabel);

    Switch sw = new Switch();
    sw.setSize(60, 30);
    sw.setOnCheckedChangeListener(new SwitchHandler(switchLabel));
    root.addView(sw);

    // -- SeekBar driving a determinate ProgressBar --
    TextView seekLabel = new TextView();
    seekLabel.setText("SeekBar: 0");
    seekLabel.setTextColor(Color.CYAN);
    seekLabel.setIncludeFontPadding(false);
    root.addView(seekLabel);

    ProgressBar progress = new ProgressBar();
    progress.setSize(200, 12);
    root.addView(progress);

    SeekBar seekBar = new SeekBar(100);
    seekBar.setSize(200, 20);
    seekBar.setOnSeekBarChangeListener(
        (bar, v, fromUser) -> {
          seekLabel.setText("SeekBar: " + v);
          progress.setProgress(v);
        });
    root.addView(seekBar);

    // -- Indeterminate spinner --
    TextView spinnerProgressLabel = new TextView();
    spinnerProgressLabel.setText("Working...");
    spinnerProgressLabel.setTextColor(Color.WHITE);
    spinnerProgressLabel.setIncludeFontPadding(false);
    root.addView(spinnerProgressLabel);

    ProgressBar busy = ProgressBar.indeterminate();
    busy.setSize(40, 40);
    root.addView(busy);

    // -- ImageView showing a bundled PNG asset --
    ImageView img = new ImageView();
    img.setSize(48, 48);
    img.setImageSource("logo.png");
    img.setScaleType(ImageView.SCALE_FIT_CENTER);
    root.addView(img);

    // -- CheckBox --
    CheckBox checkBox = new CheckBox();
    checkBox.setText("Enable notifications");
    checkBox.setOnCheckedChangeListener(
        (view, checked) -> {
          if (checked) {
            Log.i("DisplayDemo", "Notifications enabled");
          } else {
            Log.i("DisplayDemo", "Notifications disabled");
          }
        });
    root.addView(checkBox);

    // -- Spinner --
    TextView spinnerLabel = new TextView();
    spinnerLabel.setText("Color: Red");
    spinnerLabel.setTextColor(Color.RED);
    spinnerLabel.setIncludeFontPadding(false);
    root.addView(spinnerLabel);

    Spinner spinner = new Spinner();
    spinner.setItems("Red\nGreen\nBlue\nYellow");
    spinner.setSize(200, 40);
    spinner.setOnItemSelectedListener(
        (parent, pos) -> {
          if (pos == 0) {
            spinnerLabel.setText("Color: Red");
            spinnerLabel.setTextColor(Color.RED);
          } else if (pos == 1) {
            spinnerLabel.setText("Color: Green");
            spinnerLabel.setTextColor(Color.GREEN);
          } else if (pos == 2) {
            spinnerLabel.setText("Color: Blue");
            spinnerLabel.setTextColor(Color.BLUE);
          } else {
            spinnerLabel.setText("Color: Yellow");
            spinnerLabel.setTextColor(Color.YELLOW);
          }
        });
    root.addView(spinner);

    setContentView(scroll);
  }
}
