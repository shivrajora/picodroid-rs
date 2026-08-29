// SPDX-License-Identifier: GPL-3.0-only
package picodroid.inject.compiler;

import static org.junit.Assert.assertFalse;
import static org.junit.Assert.assertTrue;
import static picodroid.inject.compiler.CompileHarness.compile;
import static picodroid.inject.compiler.CompileHarness.src;

import org.junit.Test;
import picodroid.inject.compiler.CompileHarness.Result;
import picodroid.inject.compiler.CompileHarness.Source;

/** One test per compile-time rule; each asserts the failure and that nothing was generated. */
public class ValidationTest {
  private static final Source CLOCK =
      src(
          "t.Clock",
          "package t;",
          "public class Clock {",
          "  @javax.inject.Inject public Clock() {}",
          "}");

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
    assertTrue(
        "nothing should be generated on error: " + r.generated.keySet(), r.generated.isEmpty());
  }

  @Test
  public void twoInjectConstructors() throws Exception {
    assertError(
        "Types may only contain one @Inject constructor",
        src(
            "t.Two",
            "package t;",
            "public class Two {",
            "  @javax.inject.Inject Two() {}",
            "  @javax.inject.Inject Two(int x) {}",
            "}"));
  }

  @Test
  public void injectConstructorOnFrameworkComponent() throws Exception {
    assertError(
        "is instantiated by the framework and must keep a no-arg constructor",
        CLOCK,
        src(
            "t.Home",
            "package t;",
            "public class Home extends picodroid.app.Activity {",
            "  @javax.inject.Inject Home(Clock c) {}",
            "}"));
  }

  @Test
  public void injectConstructorOnAbstractClass() throws Exception {
    assertError(
        "Abstract classes cannot have an @Inject constructor",
        src(
            "t.A",
            "package t;",
            "public abstract class A {",
            "  @javax.inject.Inject A() {}",
            "}"));
  }

  @Test
  public void privateInjectConstructor() throws Exception {
    assertError(
        "@Inject constructors must not be private",
        src("t.P", "package t;", "public class P {", "  @javax.inject.Inject private P() {}", "}"));
  }

  @Test
  public void privateField() throws Exception {
    assertError(
        "@Inject fields must not be private",
        CLOCK,
        src(
            "t.F",
            "package t;",
            "public class F {",
            "  @javax.inject.Inject private Clock c;",
            "}"));
  }

  @Test
  public void finalField() throws Exception {
    assertError(
        "@Inject fields must not be final",
        CLOCK,
        src(
            "t.F",
            "package t;",
            "public class F {",
            "  @javax.inject.Inject final Clock c = null;",
            "}"));
  }

  @Test
  public void staticField() throws Exception {
    assertError(
        "@Inject fields must not be static",
        CLOCK,
        src(
            "t.F",
            "package t;",
            "public class F {",
            "  @javax.inject.Inject static Clock c;",
            "}"));
  }

  @Test
  public void privateMethod() throws Exception {
    assertError(
        "@Inject methods must not be private",
        CLOCK,
        src(
            "t.M",
            "package t;",
            "public class M {",
            "  @javax.inject.Inject private void set(Clock c) {}",
            "}"));
  }

  @Test
  public void staticMethod() throws Exception {
    assertError(
        "@Inject methods must not be static",
        CLOCK,
        src(
            "t.M",
            "package t;",
            "public class M {",
            "  @javax.inject.Inject static void set(Clock c) {}",
            "}"));
  }

  @Test
  public void genericMethod() throws Exception {
    assertError(
        "@Inject methods must not be generic",
        CLOCK,
        src(
            "t.M",
            "package t;",
            "public class M {",
            "  @javax.inject.Inject <T> void set(Clock c) {}",
            "}"));
  }

  @Test
  public void primitiveDependency() throws Exception {
    assertError(
        "Primitive types cannot be injected",
        src("t.P", "package t;", "public class P {", "  @javax.inject.Inject P(int x) {}", "}"));
  }

  @Test
  public void arrayDependency() throws Exception {
    assertError(
        "Arrays cannot be injected",
        CLOCK,
        src(
            "t.P",
            "package t;",
            "public class P {",
            "  @javax.inject.Inject P(Clock[] x) {}",
            "}"));
  }

  @Test
  public void parameterizedDependency() throws Exception {
    assertError(
        "Parameterized types cannot be injected",
        src(
            "t.P",
            "package t;",
            "public class P {",
            "  @javax.inject.Inject P(java.util.List<String> x) {}",
            "}"));
  }

  @Test
  public void nestedProviderRejected() throws Exception {
    assertError(
        "Nested Provider/Lazy are not supported",
        CLOCK,
        src(
            "t.P",
            "package t;",
            "public class P {",
            "  @javax.inject.Inject P(javax.inject.Provider<picodroid.di.Lazy<Clock>> x) {}",
            "}"));
  }

  @Test
  public void rawProviderRejected() throws Exception {
    assertError(
        "Raw javax.inject.Provider cannot be injected",
        CLOCK,
        src(
            "t.P",
            "package t;",
            "public class P {",
            "  @javax.inject.Inject P(javax.inject.Provider x) {}",
            "}"));
  }

  @Test
  public void providerOfUnprovidableRejected() throws Exception {
    assertError(
        "t.Plain cannot be provided without an @Inject constructor",
        src("t.Plain", "package t;", "public class Plain {}"),
        src(
            "t.P",
            "package t;",
            "public class P {",
            "  @javax.inject.Inject P(picodroid.di.Lazy<Plain> x) {}",
            "}"));
  }

  @Test
  public void providerOfWildcardRejected() throws Exception {
    assertError(
        "Type variables cannot be injected",
        CLOCK,
        src(
            "t.P",
            "package t;",
            "public class P {",
            "  @javax.inject.Inject P(javax.inject.Provider<? extends Clock> x) {}",
            "}"));
  }

  @Test
  public void interfaceDependency() throws Exception {
    assertError(
        "is not a class and cannot be provided without an @Inject constructor",
        src("t.I", "package t;", "public interface I {}"),
        src("t.P", "package t;", "public class P {", "  @javax.inject.Inject P(I i) {}", "}"));
  }

  @Test
  public void abstractDependency() throws Exception {
    assertError(
        "is abstract and cannot be provided without an @Inject constructor",
        src("t.A", "package t;", "public abstract class A {}"),
        src("t.P", "package t;", "public class P {", "  @javax.inject.Inject P(A a) {}", "}"));
  }

  @Test
  public void unresolvableDependency() throws Exception {
    assertError(
        "t.Plain cannot be provided without an @Inject constructor",
        src("t.Plain", "package t;", "public class Plain {}"),
        src("t.P", "package t;", "public class P {", "  @javax.inject.Inject Plain plain;", "}"));
  }

  @Test
  public void constructorCycle() throws Exception {
    assertError(
        "Found a dependency cycle: A -> B -> A",
        src("t.A", "package t;", "public class A {", "  @javax.inject.Inject A(B b) {}", "}"),
        src("t.B", "package t;", "public class B {", "  @javax.inject.Inject B(A a) {}", "}"));
  }

  @Test
  public void memberInjectionCycleThroughSingleton() throws Exception {
    assertError(
        "Found a dependency cycle",
        src(
            "t.A",
            "package t;",
            "@javax.inject.Singleton",
            "public class A {",
            "  @javax.inject.Inject A(B b) {}",
            "}"),
        src(
            "t.B",
            "package t;",
            "public class B {",
            "  @javax.inject.Inject A a;",
            "  @javax.inject.Inject B() {}",
            "}"));
  }

  @Test
  public void singletonWithoutInjectConstructor() throws Exception {
    assertError(
        "@Singleton requires an @Inject constructor",
        src("t.S", "package t;", "@javax.inject.Singleton", "public class S {}"));
  }

  @Test
  public void singletonOnMethodRejected() throws Exception {
    assertError(
        "@Singleton on a method is only supported on @Provides methods of a @Module",
        CLOCK,
        src(
            "t.M",
            "package t;",
            "public class M {",
            "  @javax.inject.Singleton Clock provide() { return null; }",
            "}"));
  }

  private static final Source GREETING =
      src("t.Greeting", "package t;", "public interface Greeting {", "  String greet();", "}");

  @Test
  public void providesOutsideModuleRejected() throws Exception {
    assertError(
        "@Provides methods can only be present within a @Module class",
        GREETING,
        src(
            "t.NotAModule",
            "package t;",
            "public class NotAModule {",
            "  @picodroid.di.Provides static Greeting g() { return null; }",
            "}"));
  }

  @Test
  public void duplicateBindingAcrossModules() throws Exception {
    assertError(
        "t.Greeting is bound multiple times",
        GREETING,
        src(
            "t.A",
            "package t;",
            "@picodroid.di.Module",
            "public final class A {",
            "  @picodroid.di.Provides static Greeting g() { return null; }",
            "}"),
        src(
            "t.B",
            "package t;",
            "@picodroid.di.Module",
            "public final class B {",
            "  @picodroid.di.Provides static Greeting g() { return null; }",
            "}"));
  }

  @Test
  public void duplicateBindingModuleAndInjectConstructor() throws Exception {
    assertError(
        "t.Clock is bound multiple times",
        CLOCK,
        src(
            "t.A",
            "package t;",
            "@picodroid.di.Module",
            "public final class A {",
            "  @picodroid.di.Provides static Clock c() { return null; }",
            "}"));
  }

  @Test
  public void providesMustReturnAValue() throws Exception {
    assertError(
        "@Provides methods must return a value",
        src(
            "t.A",
            "package t;",
            "@picodroid.di.Module",
            "public final class A {",
            "  @picodroid.di.Provides static void nothing() {}",
            "}"));
  }

  @Test
  public void providesPrimitiveRejected() throws Exception {
    assertError(
        "@Provides methods must return a class or interface type",
        src(
            "t.A",
            "package t;",
            "@picodroid.di.Module",
            "public final class A {",
            "  @picodroid.di.Provides static int n() { return 1; }",
            "}"));
  }

  @Test
  public void privateProvidesRejected() throws Exception {
    assertError(
        "@Provides methods must not be private",
        GREETING,
        src(
            "t.A",
            "package t;",
            "@picodroid.di.Module",
            "public final class A {",
            "  @picodroid.di.Provides private static Greeting g() { return null; }",
            "}"));
  }

  @Test
  public void sameNameProvidesRejected() throws Exception {
    assertError(
        "Cannot have more than one @Provides method with the same name",
        GREETING,
        CLOCK,
        src(
            "t.A",
            "package t;",
            "@picodroid.di.Module",
            "public final class A {",
            "  @picodroid.di.Provides static Greeting g() { return null; }",
            "  @picodroid.di.Provides static Clock g(Greeting x) { return null; }",
            "}"));
  }

  @Test
  public void instanceProvidesOnAbstractModuleRejected() throws Exception {
    assertError(
        "must be concrete",
        GREETING,
        src(
            "t.A",
            "package t;",
            "@picodroid.di.Module",
            "public abstract class A {",
            "  @picodroid.di.Provides Greeting g() { return null; }",
            "}"));
  }

  @Test
  public void instanceProvidesNeedsNoArgConstructor() throws Exception {
    assertError(
        "needs a non-private no-arg constructor",
        GREETING,
        src(
            "t.A",
            "package t;",
            "@picodroid.di.Module",
            "public final class A {",
            "  A(int x) {}",
            "  @picodroid.di.Provides Greeting g() { return null; }",
            "}"));
  }

  @Test
  public void injectConstructorOnModuleRejected() throws Exception {
    assertError(
        "@Module classes cannot have an @Inject constructor",
        GREETING,
        src(
            "t.A",
            "package t;",
            "@picodroid.di.Module",
            "public final class A {",
            "  @javax.inject.Inject A() {}",
            "  @picodroid.di.Provides static Greeting g() { return null; }",
            "}"));
  }

  @Test
  public void cycleThroughProvides() throws Exception {
    assertError(
        "Found a dependency cycle",
        GREETING,
        src(
            "t.A",
            "package t;",
            "@picodroid.di.Module",
            "public final class A {",
            "  @picodroid.di.Provides static Greeting g(B b) { return null; }",
            "}"),
        src(
            "t.B",
            "package t;",
            "public class B {",
            "  @javax.inject.Inject B(Greeting g) {}",
            "}"));
  }

  @Test
  public void customScopeRejected() throws Exception {
    assertError(
        "Only @Singleton is supported as a scope",
        src(
            "t.ActivityScoped",
            "package t;",
            "@javax.inject.Scope",
            "@java.lang.annotation.Retention(java.lang.annotation.RetentionPolicy.SOURCE)",
            "public @interface ActivityScoped {}"),
        src(
            "t.S",
            "package t;",
            "@ActivityScoped",
            "public class S {",
            "  @javax.inject.Inject S() {}",
            "}"));
  }

  @Test
  public void shadowedFieldInSuperclass() throws Exception {
    assertError(
        "collides with field 'clock' declared in t.Base",
        CLOCK,
        src("t.Base", "package t;", "public class Base {", "  Object clock;", "}"),
        src(
            "t.Leaf",
            "package t;",
            "public class Leaf extends Base {",
            "  @javax.inject.Inject Clock clock;",
            "}"));
  }

  @Test
  public void shadowedFieldInSubclass() throws Exception {
    assertError(
        "collides with field 'clock' declared in t.Leaf",
        CLOCK,
        src(
            "t.Base",
            "package t;",
            "public class Base {",
            "  @javax.inject.Inject Clock clock;",
            "}"),
        src("t.Leaf", "package t;", "public class Leaf extends Base {", "  Object clock;", "}"));
  }

  @Test
  public void innerClassRejected() throws Exception {
    assertError(
        "Nested classes using @Inject must be static",
        src(
            "t.Outer",
            "package t;",
            "public class Outer {",
            "  public class Inner {",
            "    @javax.inject.Inject Inner() {}",
            "  }",
            "}"));
  }

  @Test
  public void genericClassRejected() throws Exception {
    assertError(
        "Generic classes are not supported by @Inject",
        src("t.G", "package t;", "public class G<T> {", "  @javax.inject.Inject G() {}", "}"));
  }

  @Test
  public void injectOnInterfaceRejected() throws Exception {
    assertError(
        "can only be used on classes",
        src("t.I", "package t;", "@javax.inject.Singleton", "public interface I {}"));
  }
}
