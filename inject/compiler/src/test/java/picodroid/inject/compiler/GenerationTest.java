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

/** Golden-output tests: the generated sources are part of the contract, byte for byte. */
public class GenerationTest {
  private static final String HEADER = SourceWriter.HEADER + "package t;\n\n";

  private static void assertClean(Result r) {
    assertTrue("compile failed: " + r.errors(), r.success);
    assertEquals(Collections.emptyList(), r.errors());
  }

  @Test
  public void unscopedFactory() throws Exception {
    Result r =
        compile(
            src(
                "t.Clock",
                "package t;",
                "public class Clock {",
                "  @javax.inject.Inject public Clock() {}",
                "}"),
            src(
                "t.Greeter",
                "package t;",
                "public class Greeter {",
                "  final Clock clock;",
                "  @javax.inject.Inject Greeter(Clock clock) { this.clock = clock; }",
                "}"));
    assertClean(r);
    assertEquals(
        HEADER
            + "public final class Greeter_Factory {\n"
            + "  private Greeter_Factory() {}\n"
            + "\n"
            + "  public static t.Greeter get() {\n"
            + "    return new t.Greeter(t.Clock_Factory.get());\n"
            + "  }\n"
            + "}\n",
        r.generated("t.Greeter_Factory"));
    assertFalse(r.generated.containsKey("t.Greeter_MembersInjector"));
  }

  @Test
  public void singletonFactory() throws Exception {
    Result r =
        compile(
            src(
                "t.Clock",
                "package t;",
                "@javax.inject.Singleton",
                "public class Clock {",
                "  @javax.inject.Inject public Clock() {}",
                "}"));
    assertClean(r);
    assertEquals(
        HEADER
            + "public final class Clock_Factory {\n"
            + "  private static t.Clock instance;\n"
            + "\n"
            + "  private Clock_Factory() {}\n"
            + "\n"
            + "  public static t.Clock get() {\n"
            + "    t.Clock local = instance;\n"
            + "    if (local == null) {\n"
            + "      synchronized (Clock_Factory.class) {\n"
            + "        local = instance;\n"
            + "        if (local == null) {\n"
            + "          local = new t.Clock();\n"
            + "          instance = local;\n"
            + "        }\n"
            + "      }\n"
            + "    }\n"
            + "    return local;\n"
            + "  }\n"
            + "}\n",
        r.generated("t.Clock_Factory"));
  }

  @Test
  public void factoryWithMembersInjectsBeforeReturn() throws Exception {
    Result r =
        compile(
            src(
                "t.Clock",
                "package t;",
                "public class Clock {",
                "  @javax.inject.Inject public Clock() {}",
                "}"),
            src(
                "t.Message",
                "package t;",
                "@javax.inject.Singleton",
                "public class Message {",
                "  @javax.inject.Inject Clock field;",
                "  Clock viaMethod;",
                "  @javax.inject.Inject Message(Clock c) {}",
                "  @javax.inject.Inject void setClock(Clock c) { viaMethod = c; }",
                "}"));
    assertClean(r);
    String factory = r.generated("t.Message_Factory");
    assertTrue(
        factory,
        factory.contains(
            "          local = new t.Message(t.Clock_Factory.get());\n"
                + "          t.Message_MembersInjector.injectMembers(local);\n"
                + "          instance = local;\n"));
    assertEquals(
        HEADER
            + "public final class Message_MembersInjector {\n"
            + "  private Message_MembersInjector() {}\n"
            + "\n"
            + "  public static void injectMembers(t.Message instance) {\n"
            + "    instance.field = t.Clock_Factory.get();\n"
            + "    instance.setClock(t.Clock_Factory.get());\n"
            + "  }\n"
            + "}\n",
        r.generated("t.Message_MembersInjector"));
  }

  @Test
  public void injectorDelegatesToNearestInjectableSuperclass() throws Exception {
    Result r =
        compile(
            src(
                "t.Clock",
                "package t;",
                "public class Clock {",
                "  @javax.inject.Inject public Clock() {}",
                "}"),
            src(
                "t.Base",
                "package t;",
                "public class Base {",
                "  @javax.inject.Inject Clock clock;",
                "}"),
            src("t.Middle", "package t;", "public class Middle extends Base {}"),
            src(
                "t.Leaf",
                "package t;",
                "public class Leaf extends Middle {",
                "  @javax.inject.Inject Clock other;",
                "  @javax.inject.Inject Leaf() {}",
                "}"));
    assertClean(r);
    assertEquals(
        HEADER
            + "public final class Leaf_MembersInjector {\n"
            + "  private Leaf_MembersInjector() {}\n"
            + "\n"
            + "  public static void injectMembers(t.Leaf instance) {\n"
            + "    t.Base_MembersInjector.injectMembers(instance);\n"
            + "    instance.other = t.Clock_Factory.get();\n"
            + "  }\n"
            + "}\n",
        r.generated("t.Leaf_MembersInjector"));
    assertFalse(
        "Middle has no members of its own", r.generated.containsKey("t.Middle_MembersInjector"));
  }

  @Test
  public void factoryUsesAncestorInjectorWhenLeafHasNoMembers() throws Exception {
    Result r =
        compile(
            src(
                "t.Clock",
                "package t;",
                "public class Clock {",
                "  @javax.inject.Inject public Clock() {}",
                "}"),
            src(
                "t.Base",
                "package t;",
                "public class Base {",
                "  @javax.inject.Inject Clock clock;",
                "}"),
            src(
                "t.Sub",
                "package t;",
                "public class Sub extends Base {",
                "  @javax.inject.Inject Sub() {}",
                "}"));
    assertClean(r);
    assertEquals(
        HEADER
            + "public final class Sub_Factory {\n"
            + "  private Sub_Factory() {}\n"
            + "\n"
            + "  public static t.Sub get() {\n"
            + "    t.Sub instance = new t.Sub();\n"
            + "    t.Base_MembersInjector.injectMembers(instance);\n"
            + "    return instance;\n"
            + "  }\n"
            + "}\n",
        r.generated("t.Sub_Factory"));
    assertFalse(r.generated.containsKey("t.Sub_MembersInjector"));
  }

  @Test
  public void frameworkLeafGetsInjectorEvenWithoutOwnMembers() throws Exception {
    Result r =
        compile(
            src(
                "t.Clock",
                "package t;",
                "public class Clock {",
                "  @javax.inject.Inject public Clock() {}",
                "}"),
            src(
                "t.BaseActivity",
                "package t;",
                "public abstract class BaseActivity extends picodroid.app.Activity {",
                "  @javax.inject.Inject Clock clock;",
                "}"),
            src("t.Detail", "package t;", "public class Detail extends BaseActivity {}"),
            src(
                "t.AbstractDetail",
                "package t;",
                "public abstract class AbstractDetail extends BaseActivity {}"));
    assertClean(r);
    assertEquals(
        HEADER
            + "public final class Detail_MembersInjector {\n"
            + "  private Detail_MembersInjector() {}\n"
            + "\n"
            + "  public static void injectMembers(t.Detail instance) {\n"
            + "    t.BaseActivity_MembersInjector.injectMembers(instance);\n"
            + "  }\n"
            + "}\n",
        r.generated("t.Detail_MembersInjector"));
    assertTrue(r.generated.containsKey("t.BaseActivity_MembersInjector"));
    assertFalse(
        "abstract components get no leaf injector",
        r.generated.containsKey("t.AbstractDetail_MembersInjector"));
  }

  @Test
  public void nestedClassesUseFlatNames() throws Exception {
    Result r =
        compile(
            src(
                "t.Outer",
                "package t;",
                "public class Outer {",
                "  public static class Inner {",
                "    @javax.inject.Inject Inner() {}",
                "  }",
                "  public static class Holder {",
                "    @javax.inject.Inject Inner inner;",
                "  }",
                "}"));
    assertClean(r);
    assertTrue(r.generated("t.Outer_Inner_Factory").contains("    return new t.Outer.Inner();\n"));
    assertTrue(
        r.generated("t.Outer_Holder_MembersInjector")
            .contains("    instance.inner = t.Outer_Inner_Factory.get();\n"));
  }

  @Test
  public void crossPackageDependenciesUseQualifiedFactories() throws Exception {
    Result r =
        compile(
            src(
                "a.Clock",
                "package a;",
                "public class Clock {",
                "  @javax.inject.Inject public Clock() {}",
                "}"),
            src(
                "b.Greeter",
                "package b;",
                "public class Greeter {",
                "  @javax.inject.Inject public Greeter(a.Clock clock) {}",
                "}"));
    assertClean(r);
    assertTrue(r.generated("b.Greeter_Factory").contains("new b.Greeter(a.Clock_Factory.get())"));
  }

  @Test
  public void noAnnotationsGeneratesNothing() throws Exception {
    Result r = compile(src("t.Plain", "package t;", "public class Plain {}"));
    assertClean(r);
    assertTrue(r.generated.isEmpty());
  }
}
