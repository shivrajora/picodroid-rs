package picodroid.app;

import picodroid.graphics.Display;
import picodroid.view.View;

public class Activity {
  public void onCreate() {
    // Subclass overrides this
  }

  public void setContentView(View root) {
    Display.getInstance().setContentView(root);
  }

  public Display getDisplay() {
    return Display.getInstance();
  }
}
