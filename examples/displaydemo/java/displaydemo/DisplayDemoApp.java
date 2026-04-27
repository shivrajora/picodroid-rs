package displaydemo;

import picodroid.app.Application;
import picodroid.content.Intent;

public class DisplayDemoApp extends Application {
  public void onCreate() {
    startActivity(new Intent(DisplayDemoActivity.class));
  }
}
