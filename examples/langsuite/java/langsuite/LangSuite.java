// SPDX-License-Identifier: GPL-3.0-only
package langsuite;

import picodroid.app.Application;
import picodroid.util.Log;

/**
 * Aggregates 13 standalone term-style demos into one PAPK so HIL can verify them with a single
 * build+flash cycle instead of 13. Each sub-demo is invoked via its public {@code static run()};
 * exceptions are caught per-demo so a single failure doesn't suppress the patterns of every demo
 * that follows it.
 */
public class LangSuite extends Application {
  private static final String TAG = "LangSuite";

  public void onCreate() {
    Log.i(TAG, "=== LangSuite start ===");

    safe("anondemo", () -> anondemo.AnonDemo.run());
    safe("bytecodecoverage", () -> bytecodecoverage.BytecodeCoverage.run());
    safe("collectionsdemo", () -> collectionsdemo.CollectionsDemo.run());
    safe("enumdemo", () -> enumdemo.EnumDemo.run());
    safe("exceptiondemo", () -> exceptiondemo.ExceptionDemo.run());
    safe("floatdemo", () -> floatdemo.FloatDemo.run());
    safe("inherit", () -> inherit.InheritDemo.run());
    safe("interfacedemo", () -> interfacedemo.InterfaceDemo.run());
    safe("lambdademo", () -> lambdademo.LambdaDemo.run());
    safe("mathsdemo", () -> mathsdemo.MathsDemo.run());
    safe("stringdemo", () -> stringdemo.StringDemo.run());
    safe("syncdemo", () -> syncdemo.SyncDemo.run());
    safe("trywithresourcesdemo", () -> trywithresourcesdemo.TryWithResourcesDemo.run());

    Log.i(TAG, "=== LangSuite done ===");
  }

  private static void safe(String name, Runnable demo) {
    try {
      demo.run();
    } catch (Throwable t) {
      Log.i(TAG, name + " threw: " + t);
    }
  }
}
