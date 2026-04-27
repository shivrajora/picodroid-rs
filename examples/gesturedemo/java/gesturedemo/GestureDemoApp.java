package gesturedemo;

import picodroid.app.Application;
import picodroid.content.Intent;

public class GestureDemoApp extends Application {
  public void onCreate() {
    new GestureAppComponent();
    startActivity(new Intent(GestureDemoActivity.class));
  }
}
