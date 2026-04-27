package dialogdemo;

import picodroid.app.Application;
import picodroid.content.Intent;

public class DialogDemoApp extends Application {
  public void onCreate() {
    startActivity(new Intent(DialogDemoActivity.class));
  }
}
