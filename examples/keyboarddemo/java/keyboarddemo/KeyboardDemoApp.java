package keyboarddemo;

import picodroid.app.Application;

public class KeyboardDemoApp extends Application {
  public void onCreate() {
    startActivity(new KeyboardDemoActivity());
  }
}
