// SPDX-License-Identifier: GPL-3.0-only
package picodroid.inject.compiler;

import javax.lang.model.element.AnnotationMirror;
import javax.lang.model.element.Element;
import javax.lang.model.element.ElementKind;
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

  static String qualifiedNameOf(AnnotationMirror am) {
    return ((TypeElement) am.getAnnotationType().asElement()).getQualifiedName().toString();
  }
}
