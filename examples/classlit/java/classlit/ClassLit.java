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
    // repeat calls hand back the same string. App classes are never
    // renamed, so the literal holds in both build modes.
    Log.i("ClassLit", "classlit.ClassLit".equals(a.getName()) ? "dot-form ok" : "dot-form WRONG");
    // Framework and java/** classes ARE renamed under --shrink (ProGuard
    // semantics: getName() returns the mapped name, e.g. "b.AQ"), so those
    // checks compare spellings for consistency and dot-form rather than
    // against a literal: the String class object and a String instance's
    // getClass() must agree, and neither may carry a slash.
    String strName = "x".getClass().getName();
    Log.i(
        "ClassLit",
        strName.equals(String.class.getName()) && strName.indexOf('/') < 0
            ? "str ok"
            : "str WRONG");

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
        sc.getName().equals(strName) && sc.getName().length() > 0
            ? "builtin literal ok"
            : "builtin literal WRONG");
    Log.i("ClassLit", sc == String.class ? "builtin literal same" : "builtin literal diff");
    Log.i(
        "ClassLit",
        "x".getClass() == String.class ? "getClass==builtin literal" : "getClass!=builtin");
    Class<Runnable> rc = Runnable.class;
    String rcName = rc.getName();
    Log.i(
        "ClassLit",
        rcName.length() > 0 && rcName.indexOf('/') < 0 && !rcName.equals(sc.getName())
            ? "iface literal ok"
            : "iface literal WRONG");
  }
}
