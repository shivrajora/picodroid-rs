// SPDX-License-Identifier: GPL-3.0-only
package classlit;

import picodroid.app.Application;
import picodroid.util.Log;

public class ClassLit extends Application {
  @Override
  public void onCreate() {
    Class<ClassLit> a = ClassLit.class;
    Class<ClassLit> b = ClassLit.class;
    Log.i("ClassLit", "name=" + a.getName());
    Log.i("ClassLit", a == b ? "same" : "diff");

    // Object.getClass(): name readback, ldc identity, and the String receiver.
    Log.i("ClassLit", "getClass name=" + this.getClass().getName());
    Log.i("ClassLit", this.getClass() == ClassLit.class ? "getClass==literal" : "getClass diff");
    Object boxed = "text";
    Log.i("ClassLit", "string getClass=" + boxed.getClass().getName());
  }
}
