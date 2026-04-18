package keydemo;

import picodroid.app.Application;

public class KeyDemoApp extends Application {
  public void onCreate() {
    startActivity(new KeyDemoActivity());
  }
}
