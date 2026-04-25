package gesturedemo;

import picodroid.app.Application;

public class GestureDemoApp extends Application {
  public void onCreate() {
    startActivity(new GestureDemoActivity());
  }
}
