package snackbardemo;

import picodroid.app.Application;
import picodroid.content.Intent;

public class SnackbarDemoApp extends Application {
  public void onCreate() {
    startActivity(new Intent(SnackbarDemoActivity.class));
  }
}
