package dragdemo;

import picodroid.app.Application;
import picodroid.content.Intent;

public class DragDemoApp extends Application {
  public void onCreate() {
    startActivity(new Intent(DragDemoActivity.class));
  }
}
