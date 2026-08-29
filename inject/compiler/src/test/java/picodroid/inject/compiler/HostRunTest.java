// SPDX-License-Identifier: GPL-3.0-only
package picodroid.inject.compiler;

import static org.junit.Assert.assertEquals;
import static org.junit.Assert.assertNotNull;
import static org.junit.Assert.assertNotSame;
import static org.junit.Assert.assertSame;
import static org.junit.Assert.assertTrue;
import static picodroid.inject.compiler.CompileHarness.compile;
import static picodroid.inject.compiler.CompileHarness.src;

import java.net.URL;
import java.net.URLClassLoader;
import java.util.Arrays;
import org.junit.Test;
import picodroid.inject.compiler.CompileHarness.Result;

/**
 * Compiles a POJO-only graph and runs the generated factories on the host JVM: singleton identity,
 * fresh unscoped instances, and superclass-before-subclass, fields-before-methods injection order.
 */
public class HostRunTest {
  @Test
  public void generatedGraphRunsOnTheHost() throws Exception {
    Result r =
        compile(
            src(
                "t.Trace",
                "package t;",
                "public class Trace {",
                "  public static final java.util.List<String> LOG = new java.util.ArrayList<>();",
                "}"),
            src(
                "t.Clock",
                "package t;",
                "@javax.inject.Singleton",
                "public class Clock {",
                "  public static int created;",
                "  @javax.inject.Inject public Clock() { created++; }",
                "}"),
            src(
                "t.Greeter",
                "package t;",
                "public class Greeter {",
                "  public final Clock clock;",
                "  @javax.inject.Inject public Greeter(Clock clock) { this.clock = clock; }",
                "}"),
            src(
                "t.Base",
                "package t;",
                "public class Base {",
                "  @javax.inject.Inject Clock baseClock;",
                "  @javax.inject.Inject void baseMethod(Clock c) { Trace.LOG.add(\"base:\" + (c =="
                    + " baseClock)); }",
                "}"),
            src(
                "t.Leaf",
                "package t;",
                "public class Leaf extends Base {",
                "  @javax.inject.Inject Greeter greeter;",
                "  @javax.inject.Inject javax.inject.Provider<Greeter> greeters;",
                "  @javax.inject.Inject picodroid.di.Lazy<Clock> lazyClock;",
                "  @javax.inject.Inject public Leaf() {}",
                "  @javax.inject.Inject void leafMethod(Greeter g) { Trace.LOG.add(\"leaf:\" + (g"
                    + " != greeter)); }",
                "  public Clock baseClock() { return baseClock; }",
                "  public Greeter greeter() { return greeter; }",
                "  public javax.inject.Provider<Greeter> greeters() { return greeters; }",
                "  public picodroid.di.Lazy<Clock> lazyClock() { return lazyClock; }",
                "}"));
    assertTrue("compile failed: " + r.errors(), r.success);

    try (URLClassLoader loader =
        new URLClassLoader(new URL[] {r.classesDir.toUri().toURL()}, getClass().getClassLoader())) {
      Object clock1 = loader.loadClass("t.Clock_Factory").getMethod("get").invoke(null);
      Object clock2 = loader.loadClass("t.Clock_Factory").getMethod("get").invoke(null);
      assertSame(clock1, clock2);
      assertEquals(1, loader.loadClass("t.Clock").getField("created").get(null));

      Object g1 = loader.loadClass("t.Greeter_Factory").getMethod("get").invoke(null);
      Object g2 = loader.loadClass("t.Greeter_Factory").getMethod("get").invoke(null);
      assertNotSame(g1, g2);
      assertSame(clock1, loader.loadClass("t.Greeter").getField("clock").get(g1));

      Class<?> leafClass = loader.loadClass("t.Leaf");
      Object leaf = loader.loadClass("t.Leaf_Factory").getMethod("get").invoke(null);
      assertSame(clock1, leafClass.getMethod("baseClock").invoke(leaf));
      assertNotNull(leafClass.getMethod("greeter").invoke(leaf));
      Object greeters = leafClass.getMethod("greeters").invoke(leaf);
      java.lang.reflect.Method providerGet = greeters.getClass().getMethod("get");
      assertNotSame(
          "unscoped: fresh per get()", providerGet.invoke(greeters), providerGet.invoke(greeters));
      Object lazyClock = leafClass.getMethod("lazyClock").invoke(leaf);
      java.lang.reflect.Method lazyGet = lazyClock.getClass().getMethod("get");
      assertSame(clock1, lazyGet.invoke(lazyClock));
      assertSame(lazyGet.invoke(lazyClock), lazyGet.invoke(lazyClock));
      Object log = loader.loadClass("t.Trace").getField("LOG").get(null);
      assertEquals(Arrays.asList("base:true", "leaf:true"), log);
    }
  }
}
