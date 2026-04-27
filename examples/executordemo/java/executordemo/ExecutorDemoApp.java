package executordemo;

import picodroid.app.Application;
import picodroid.content.Intent;

public class ExecutorDemoApp extends Application {
  public void onCreate() {
    startActivity(new Intent(ExecutorDemoActivity.class));
  }
}
