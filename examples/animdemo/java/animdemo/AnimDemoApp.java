package animdemo;

import picodroid.app.Application;

public class AnimDemoApp extends Application {
  public void onCreate() {
    startActivity(new AnimDemoActivity());
  }
}
