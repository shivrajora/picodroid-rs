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
    // Java spec: getName() returns the dot-form binary name, cached so
    // repeat calls hand back the same string.
    Log.i("ClassLit", "classlit.ClassLit".equals(a.getName()) ? "dot-form ok" : "dot-form WRONG");
    Log.i("ClassLit", "java.lang.String".equals("x".getClass().getName()) ? "str ok" : "str WRONG");

    // Object.getClass(): name readback, ldc identity, and the String receiver.
    Log.i("ClassLit", "getClass name=" + this.getClass().getName());
    Log.i("ClassLit", this.getClass() == ClassLit.class ? "getClass==literal" : "getClass diff");
    Object boxed = "text";
    Log.i("ClassLit", "string getClass=" + boxed.getClass().getName());

    // Class literals on the classfile-less builtins. `java.lang.String` and
    // `java.lang.Runnable` are served natively and ship no .class file, so
    // `ldc` resolves them through BUILTIN_CLASS_NAMES rather than the loaded
    // class table.
    Class<String> sc = String.class;
    Log.i(
        "ClassLit",
        "java.lang.String".equals(sc.getName()) ? "builtin literal ok" : "builtin literal WRONG");
    Log.i("ClassLit", sc == String.class ? "builtin literal same" : "builtin literal diff");
    Log.i(
        "ClassLit",
        "x".getClass() == String.class ? "getClass==builtin literal" : "getClass!=builtin");
    Class<Runnable> rc = Runnable.class;
    Log.i(
        "ClassLit",
        "java.lang.Runnable".equals(rc.getName()) ? "iface literal ok" : "iface literal WRONG");
  }
}
