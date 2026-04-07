package displaydemo;

import picodroid.app.Activity;
import picodroid.graphics.Color;
import picodroid.pio.Gpio;
import picodroid.pio.PeripheralManager;
import picodroid.util.Log;
import picodroid.widget.Button;
import picodroid.widget.LinearLayout;
import picodroid.widget.Switch;
import picodroid.widget.TextView;
import picodroid.widget.ToggleButton;

public class DisplayDemoActivity extends Activity {
  public void onCreate() {
    getDisplay().calibrate();
    Log.i("DisplayDemo", "Display ready");

    LinearLayout root = new LinearLayout();
    root.setOrientation(LinearLayout.VERTICAL);
    root.setSize(320, 240);

    TextView title = new TextView();
    title.setText("Picodroid UI Demo");
    title.setTextColor(Color.WHITE);
    root.addView(title);

    Button btn = new Button("Tap Me!");
    btn.setSize(200, 50);
    btn.setOnClickListener(new TapHandler(title));
    root.addView(btn);

    TextView toggleLabel = new TextView();
    toggleLabel.setText("LED: OFF");
    toggleLabel.setTextColor(Color.WHITE);
    root.addView(toggleLabel);

    ToggleButton toggle = new ToggleButton("ON", "OFF");
    toggle.setSize(200, 50);
    PeripheralManager manager = PeripheralManager.getInstance();
    Gpio led = manager.openGpio("GP25");
    led.setDirection(Gpio.DIRECTION_OUT_INITIALLY_LOW);
    toggle.setOnCheckedChangeListener(
        () -> {
          if (toggle.isChecked()) {
            toggleLabel.setText("LED: ON");
            led.setValue(true);
          } else {
            toggleLabel.setText("LED: OFF");
            led.setValue(false);
          }
        });
    root.addView(toggle);

    TextView switchLabel = new TextView();
    switchLabel.setText("Switch: OFF");
    switchLabel.setTextColor(Color.WHITE);
    root.addView(switchLabel);

    Switch sw = new Switch();
    sw.setSize(60, 30);
    sw.setOnCheckedChangeListener(new SwitchHandler(switchLabel, sw));
    root.addView(sw);

    setContentView(root);
  }
}
