package executordemo;

import picodroid.app.Application;

public class ExecutorDemoApp extends Application {
  public void onCreate() {
    startActivity(new ExecutorDemoActivity());
  }
}
