package navdemo;

import picodroid.app.Application;

public class NavDemoApp extends Application {
  public void onCreate() {
    startActivity(new HomeActivity());
  }
}
