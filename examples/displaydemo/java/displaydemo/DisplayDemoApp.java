package displaydemo;

import picodroid.app.Application;

public class DisplayDemoApp extends Application {
  public void onCreate() {
    startActivity(new DisplayDemoActivity());
  }
}
