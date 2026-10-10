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
    // repeat calls hand back the same string. Under --shrink-app the app's
    // own classes are renamed too (ProGuard semantics: "c.A"), so accept
    // either the literal or a dot-form synthetic name — never a slash.
    String name = a.getName();
    Log.i(
        "ClassLit",
        "classlit.ClassLit".equals(name) || (name.startsWith("c.") && name.indexOf('/') < 0)
            ? "dot-form ok"
            : "dot-form WRONG");
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

    reflectionLite();
  }

  /** Made through {@code Class.newInstance()}: its static initialiser must run first. */
  public static final class Made {
    static int made;
    static final int SEED = seed();

    static int seed() {
      return 42;
    }

    public Made() {
      made++;
    }
  }

  /** Not constructible: abstract. */
  public abstract static class Shape {}

  /** Not constructible through {@code newInstance()}: no no-argument constructor. */
  public static final class Sized {
    public Sized(int size) {}
  }

  /**
   * Reflection-lite (docs/designs/class-newinstance-2026-10.md): {@code Class.forName} by the name
   * {@code getName()} spells (so it holds under {@code --shrink-app} too), {@code newInstance()}
   * through the no-argument constructor with the class initialised first, and the three refusals.
   */
  private static void reflectionLite() {
    try {
      Class<?> c = Class.forName(ClassLit.class.getName());
      Log.i("ClassLit", c == ClassLit.class ? "forName ok" : "forName WRONG: " + c.getName());
      Class<?> s = Class.forName(String.class.getName());
      Log.i("ClassLit", s == String.class ? "forName builtin ok" : "forName builtin WRONG");
    } catch (ClassNotFoundException e) {
      Log.i("ClassLit", "forName WRONG: " + e.getMessage());
    }
    try {
      Class.forName("classlit.Nope");
      Log.i("ClassLit", "forName miss WRONG");
    } catch (ClassNotFoundException e) {
      Log.i("ClassLit", "forName miss ok: " + e.getMessage());
    }
    try {
      Object o = Made.class.newInstance();
      Object p = Made.class.newInstance();
      Log.i(
          "ClassLit",
          o instanceof Made && p instanceof Made && o != p && Made.made == 2 && Made.SEED == 42
              ? "newInstance ok"
              : "newInstance WRONG made=" + Made.made);
    } catch (InstantiationException | IllegalAccessException e) {
      Log.i("ClassLit", "newInstance WRONG: " + e);
    }
    try {
      Object made = Shape.class.newInstance();
      Log.i("ClassLit", "abstract refused WRONG: " + made);
    } catch (InstantiationException e) {
      Log.i("ClassLit", "abstract refused ok");
    } catch (IllegalAccessException e) {
      Log.i("ClassLit", "abstract refused WRONG: access");
    }
    try {
      Object made = Sized.class.newInstance();
      Log.i("ClassLit", "no ctor refused WRONG: " + made);
    } catch (InstantiationException e) {
      Log.i("ClassLit", "no ctor refused ok");
    } catch (IllegalAccessException e) {
      Log.i("ClassLit", "no ctor refused WRONG: access");
    }
    try {
      Object made = String.class.newInstance();
      Log.i("ClassLit", "builtin refused WRONG: " + made);
    } catch (InstantiationException e) {
      Log.i("ClassLit", "builtin refused ok");
    } catch (IllegalAccessException e) {
      Log.i("ClassLit", "builtin refused WRONG: access");
    }
  }
}
