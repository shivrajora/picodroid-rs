// SPDX-License-Identifier: GPL-3.0-only
package picodroid.inject.compiler;

import java.util.List;
import javax.lang.model.element.AnnotationMirror;
import javax.lang.model.element.Element;
import javax.lang.model.element.ExecutableElement;
import javax.lang.model.element.Modifier;
import javax.lang.model.element.TypeElement;
import javax.lang.model.element.VariableElement;

/**
 * Everything the processor knows about one class: its {@code @Inject} constructor(s), fields and
 * methods (declaration order), its scope, and whether the framework instantiates it.
 */
final class Binding {
  final TypeElement type;
  final List<ExecutableElement> injectConstructors;
  final List<VariableElement> injectFields;
  final List<ExecutableElement> injectMethods;
  final boolean singleton;

  /**
   * Annotations on the type that are meta-annotated {@code @Scope} but are not {@code @Singleton}.
   */
  final List<AnnotationMirror> foreignScopes;

  /** Members (methods) carrying {@code @Singleton}, which is only meaningful on classes today. */
  final List<Element> misplacedSingletons;

  /** Subclass of {@code picodroid.app.Application} / {@code Activity} / {@code Service}. */
  final boolean frameworkComponent;

  /** Nearest superclass binding that declares {@code @Inject} members of its own; may be null. */
  Binding nearestInjectableAncestor;

  Binding(
      TypeElement type,
      List<ExecutableElement> injectConstructors,
      List<VariableElement> injectFields,
      List<ExecutableElement> injectMethods,
      boolean singleton,
      List<AnnotationMirror> foreignScopes,
      List<Element> misplacedSingletons,
      boolean frameworkComponent) {
    this.type = type;
    this.injectConstructors = injectConstructors;
    this.injectFields = injectFields;
    this.injectMethods = injectMethods;
    this.singleton = singleton;
    this.foreignScopes = foreignScopes;
    this.misplacedSingletons = misplacedSingletons;
    this.frameworkComponent = frameworkComponent;
  }

  String qualifiedName() {
    return type.getQualifiedName().toString();
  }

  boolean hasInjectConstructor() {
    return !injectConstructors.isEmpty();
  }

  ExecutableElement injectConstructor() {
    return injectConstructors.get(0);
  }

  boolean hasOwnMembers() {
    return !injectFields.isEmpty() || !injectMethods.isEmpty();
  }

  boolean isAbstract() {
    return type.getModifiers().contains(Modifier.ABSTRACT);
  }

  /**
   * A {@code _MembersInjector} is generated for every class with its own {@code @Inject} members,
   * and additionally for every concrete framework component that only inherits them — so the
   * runtime probes exactly one name (the leaf's) per component.
   */
  boolean needsMembersInjector() {
    return hasOwnMembers()
        || (frameworkComponent && !isAbstract() && nearestInjectableAncestor != null);
  }

  /** The class whose injector a factory must call after construction, or null if none. */
  TypeElement membersInjectorOwner() {
    if (needsMembersInjector()) {
      return type;
    }
    return nearestInjectableAncestor == null ? null : nearestInjectableAncestor.type;
  }
}
