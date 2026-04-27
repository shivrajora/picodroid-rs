package classlit;

import picodroid.app.Application;
import picodroid.util.Log;

public class ClassLit extends Application {
  public void onCreate() {
    Class<ClassLit> a = ClassLit.class;
    Class<ClassLit> b = ClassLit.class;
    Log.i("ClassLit", "name=" + a.getName());
    Log.i("ClassLit", a == b ? "same" : "diff");
  }
}
