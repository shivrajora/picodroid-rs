package swipedemo;

import picodroid.app.Application;
import picodroid.content.Intent;

public class SwipeDemoApp extends Application {
  public void onCreate() {
    startActivity(new Intent(SwipeDemoActivity.class));
  }
}
