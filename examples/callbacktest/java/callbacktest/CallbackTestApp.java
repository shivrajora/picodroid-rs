package callbacktest;

import picodroid.app.Application;

public class CallbackTestApp extends Application {
  public void onCreate() {
    startActivity(new CallbackTestActivity());
  }
}
