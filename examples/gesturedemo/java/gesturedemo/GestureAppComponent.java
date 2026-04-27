package gesturedemo;

import picodroid.di.ApplicationComponent;
import picodroid.util.Log;

public class GestureAppComponent extends ApplicationComponent {
  private final String tag = "GestureDemo";

  public String tag() {
    return tag;
  }

  public void info(String msg) {
    Log.i(tag, msg);
  }
}
