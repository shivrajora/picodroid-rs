// SPDX-License-Identifier: GPL-3.0-only
package picodroid.inject.compiler;

import static org.junit.Assert.assertEquals;
import static org.junit.Assert.assertFalse;
import static org.junit.Assert.assertTrue;
import static picodroid.inject.compiler.CompileHarness.compile;
import static picodroid.inject.compiler.CompileHarness.src;

import java.util.Collections;
import org.junit.Test;
import picodroid.inject.compiler.CompileHarness.Result;
import picodroid.inject.compiler.CompileHarness.Source;

/**
 * The processor as kapt runs it: over javac stubs of Kotlin declarations. Each fixture here is the
 * Java that kotlinc's stub generator emits for the shape named in the test (bodies elided), so the
 * rules Kotlin apps rely on — and the two traps the docs warn about — are pinned without a Gradle
 * or KGP dependency. End-to-end coverage is {@code examples/injectdemo_kt}.
 */
public class KotlinShapesTest {
  private static final Source CLOCK =
      src(
          "t.Clock",
          "package t;",
          "@javax.inject.Singleton",
          "public final class Clock {",
          "  @javax.inject.Inject public Clock() {}",
          "}");

  private static final Source PREFS =
      src("t.Prefs", "package t;", "public final class Prefs {", "  public Prefs() {}", "}");

  private static void assertClean(Result r) {
    assertTrue("compile failed: " + r.errors(), r.success);
    assertEquals(Collections.emptyList(), r.errors());
  }

  private static void assertError(String expected, Source... sources) throws Exception {
    Result r = compile(sources);
    assertFalse("expected a compile error containing: " + expected, r.success);
    boolean found = false;
    for (String e : r.errors()) {
      if (e.contains(expected)) {
        found = true;
      }
    }
    assertTrue("errors were: " + r.errors(), found);
  }

  /** {@code @Module object M { @Provides @JvmStatic fun providePrefs(): Prefs }}. */
  @Test
  public void objectModuleWithJvmStaticProvides() throws Exception {
    Result r =
        compile(
            CLOCK,
            PREFS,
            src(
                "t.StaticModule",
                "package t;",
                "@picodroid.di.Module",
                "public final class StaticModule {",
                "  public static final StaticModule INSTANCE = null;",
                "  private StaticModule() {}",
                "  @picodroid.di.Provides",
                "  @javax.inject.Singleton",
                "  public static final Prefs providePrefs(Clock clock) { return null; }",
                "}"),
            src(
                "t.Home",
                "package t;",
                "public final class Home extends picodroid.app.Activity {",
                "  @javax.inject.Inject public Prefs prefs;",
                "}"));
    assertClean(r);
    String factory = r.generated("t.StaticModule_ProvidePrefsFactory");
    assertTrue(
        factory, factory.contains("local = t.StaticModule.providePrefs(t.Clock_Factory.get());"));
    assertFalse(r.generated.containsKey("t.StaticModule_Factory"));
    assertTrue(
        r.generated("t.Home_MembersInjector")
            .contains("instance.prefs = t.StaticModule_ProvidePrefsFactory.get();"));
  }

  /** {@code @Module class M { @Provides @Singleton fun provideBanner(): Banner }}. */
  @Test
  public void classModuleWithInstanceProvides() throws Exception {
    Result r =
        compile(
            PREFS,
            src(
                "t.InstanceModule",
                "package t;",
                "@picodroid.di.Module",
                "public final class InstanceModule {",
                "  private int banners;",
                "  public InstanceModule() {}",
                "  @picodroid.di.Provides",
                "  @javax.inject.Singleton",
                "  public final Prefs providePrefs() { return null; }",
                "}"),
            src(
                "t.Home",
                "package t;",
                "public final class Home extends picodroid.app.Activity {",
                "  @javax.inject.Inject public Prefs prefs;",
                "}"));
    assertClean(r);
    assertTrue(
        r.generated("t.InstanceModule_ProvidePrefsFactory")
            .contains("t.InstanceModule_Factory.get().providePrefs()"));
    assertTrue(r.generated("t.InstanceModule_Factory").contains("new t.InstanceModule()"));
  }

  /**
   * {@code @Inject lateinit var clock: Clock} on a final class: a public backing field plus the
   * property accessors kotlinc always emits; the injector assigns the field directly.
   */
  @Test
  public void lateinitFieldInjection() throws Exception {
    Result r =
        compile(
            CLOCK,
            src(
                "t.Home",
                "package t;",
                "public final class Home extends picodroid.app.Activity {",
                "  @javax.inject.Inject public Clock clock;",
                "  public final Clock getClock() { return null; }",
                "  public final void setClock(Clock c) {}",
                "}"));
    assertClean(r);
    assertTrue(
        r.generated("t.Home_MembersInjector").contains("instance.clock = t.Clock_Factory.get();"));
  }

  /** {@code class Greeter @Inject constructor(val clock: Clock)} — final class, final field. */
  @Test
  public void injectConstructorOnFinalClass() throws Exception {
    Result r =
        compile(
            CLOCK,
            src(
                "t.Greeter",
                "package t;",
                "public final class Greeter {",
                "  private final Clock clock;",
                "  @javax.inject.Inject public Greeter(Clock clock) { this.clock = clock; }",
                "  public final Clock getClock() { return clock; }",
                "}"));
    assertClean(r);
    assertTrue(
        r.generated("t.Greeter_Factory").contains("return new t.Greeter(t.Clock_Factory.get());"));
  }

  /**
   * {@code @Provides} inside a {@code companion object}: kotlinc emits the method on both the outer
   * class (static, with {@code @JvmStatic}) and the {@code $Companion} class, so one copy is always
   * outside the {@code @Module} — a stray.
   */
  @Test
  public void companionProvidesIsStray() throws Exception {
    assertError(
        "@Provides methods can only be present within a @Module class",
        PREFS,
        src(
            "t.Outer",
            "package t;",
            "public final class Outer {",
            "  @picodroid.di.Provides",
            "  public static final Prefs providePrefs() { return null; }",
            "  @picodroid.di.Module",
            "  public static final class Companion {",
            "    @picodroid.di.Provides",
            "    public final Prefs providePrefs() { return null; }",
            "  }",
            "}"));
  }

  /** {@code @Module object M { @Provides fun f() }} without {@code @JvmStatic}: private ctor. */
  @Test
  public void objectModuleWithoutJvmStatic() throws Exception {
    assertError(
        "needs a non-private no-arg constructor",
        PREFS,
        src(
            "t.M",
            "package t;",
            "@picodroid.di.Module",
            "public final class M {",
            "  public static final M INSTANCE = null;",
            "  private M() {}",
            "  @picodroid.di.Provides",
            "  public final Prefs providePrefs() { return null; }",
            "}"));
  }

  /** A plain {@code @Inject var x: T?} has a private backing field — the documented trap. */
  @Test
  public void privateBackingFieldIsRejected() throws Exception {
    assertError(
        "@Inject fields must not be private",
        CLOCK,
        src(
            "t.Home",
            "package t;",
            "public final class Home extends picodroid.app.Activity {",
            "  @javax.inject.Inject private Clock clock;",
            "  public final Clock getClock() { return null; }",
            "  public final void setClock(Clock c) {}",
            "}"));
  }
}
