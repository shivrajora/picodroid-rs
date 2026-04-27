package dialogdemo;

import picodroid.di.ApplicationComponent;
import picodroid.util.Log;

public class DialogAppComponent extends ApplicationComponent {
  private final String tag = "DialogDemo";

  public String tag() {
    return tag;
  }

  public void info(String msg) {
    Log.i(tag, msg);
  }
}
