package imagedemo;

import picodroid.app.Application;
import picodroid.content.Intent;

public class ImageDemoApp extends Application {
  public void onCreate() {
    startActivity(new Intent(ImageDemoActivity.class));
  }
}
