// SPDX-License-Identifier: GPL-3.0-only
package picodroid.inject.compiler;

import javax.lang.model.element.AnnotationMirror;
import javax.lang.model.element.Element;
import javax.lang.model.element.ElementKind;
import javax.lang.model.element.ExecutableElement;
import javax.lang.model.element.PackageElement;
import javax.lang.model.element.TypeElement;

/**
 * Naming contract shared by the writers, the tests, and the Rust probe in {@code
 * picodroid-core/src/lifecycle.rs}: a class {@code pkg.Outer.Inner} gets {@code
 * pkg.Outer_Inner_Factory} / {@code pkg.Outer_Inner_MembersInjector}. The JVM-side probe takes the
 * runtime class name {@code pkg/Outer$Inner}, swaps {@code $} for {@code _}, and appends the suffix
 * — the two must stay in lock-step.
 */
final class Names {
  static final String FACTORY_SUFFIX = "_Factory";
  static final String MEMBERS_INJECTOR_SUFFIX = "_MembersInjector";
  static final String PROVIDER_SUFFIX = "_Provider";
  static final String LAZY_SUFFIX = "_Lazy";

  static final String INJECT = "javax.inject.Inject";
  static final String SINGLETON = "javax.inject.Singleton";
  static final String SCOPE = "javax.inject.Scope";
  static final String PROVIDER = "javax.inject.Provider";
  static final String LAZY = "picodroid.di.Lazy";
  static final String MODULE = "picodroid.di.Module";
  static final String PROVIDES = "picodroid.di.Provides";
  static final String QUALIFIER = "javax.inject.Qualifier";
  static final String APPLICATION_CONTEXT = "picodroid.di.ApplicationContext";
  static final String CONTEXT = "picodroid.content.Context";
  static final String APPLICATION = "picodroid.app.Application";

  /**
   * What a framework binding ({@code Application}, {@code @ApplicationContext Context}) compiles
   * to: one static call into the SDK, so the generated code carries no holder of its own.
   */
  static final String APPLICATION_EXPR =
      "picodroid.di.ApplicationContextModule.provideApplication()";

  private Names() {}

  /** {@code Outer.Inner} → {@code Outer_Inner}; top-level classes are unchanged. */
  static String flatName(TypeElement type) {
    StringBuilder sb = new StringBuilder(type.getSimpleName());
    Element e = type.getEnclosingElement();
    while (e != null && e.getKind() != ElementKind.PACKAGE) {
      sb.insert(0, '_');
      sb.insert(0, e.getSimpleName());
      e = e.getEnclosingElement();
    }
    return sb.toString();
  }

  static PackageElement packageOf(Element element) {
    Element e = element;
    while (e.getKind() != ElementKind.PACKAGE) {
      e = e.getEnclosingElement();
    }
    return (PackageElement) e;
  }

  static String generatedSimpleName(TypeElement type, String suffix) {
    return flatName(type) + suffix;
  }

  static String generatedQualifiedName(TypeElement type, String suffix) {
    String pkg = packageOf(type).getQualifiedName().toString();
    String simple = generatedSimpleName(type, suffix);
    return pkg.isEmpty() ? simple : pkg + "." + simple;
  }

  /** {@code Mod.provideFoo()} → {@code Mod_ProvideFooFactory} (Dagger convention). */
  static String providesFactorySimpleName(TypeElement module, ExecutableElement method) {
    String m = method.getSimpleName().toString();
    return flatName(module) + "_" + Character.toUpperCase(m.charAt(0)) + m.substring(1) + "Factory";
  }

  static String providesFactoryQualifiedName(TypeElement module, ExecutableElement method) {
    String pkg = packageOf(module).getQualifiedName().toString();
    String simple = providesFactorySimpleName(module, method);
    return pkg.isEmpty() ? simple : pkg + "." + simple;
  }

  /** A source-level reference to {@code type} that is valid from any package. */
  static String ref(TypeElement type) {
    return type.getQualifiedName().toString();
  }

  static boolean hasAnnotation(Element element, String qualifiedName) {
    return annotation(element, qualifiedName) != null;
  }

  static AnnotationMirror annotation(Element element, String qualifiedName) {
    for (AnnotationMirror am : element.getAnnotationMirrors()) {
      if (qualifiedNameOf(am).equals(qualifiedName)) {
        return am;
      }
    }
    return null;
  }

  /**
   * The qualifier on {@code element}: {@code @ApplicationContext} (matched by name, since its
   * SOURCE-retained {@code @Qualifier} is invisible from a class file) or any annotation that is
   * visibly meta-annotated {@code @Qualifier}; null if none.
   */
  static AnnotationMirror qualifier(Element element) {
    for (AnnotationMirror am : element.getAnnotationMirrors()) {
      if (qualifiedNameOf(am).equals(APPLICATION_CONTEXT)
          || hasAnnotation(am.getAnnotationType().asElement(), QUALIFIER)) {
        return am;
      }
    }
    return null;
  }

  static boolean isApplicationContext(AnnotationMirror am) {
    return am != null && qualifiedNameOf(am).equals(APPLICATION_CONTEXT);
  }

  static String qualifiedNameOf(AnnotationMirror am) {
    return ((TypeElement) am.getAnnotationType().asElement()).getQualifiedName().toString();
  }
}
