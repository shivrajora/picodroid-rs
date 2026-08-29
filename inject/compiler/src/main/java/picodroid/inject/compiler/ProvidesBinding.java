// SPDX-License-Identifier: GPL-3.0-only
package picodroid.inject.compiler;

import javax.lang.model.element.ExecutableElement;
import javax.lang.model.element.Modifier;
import javax.lang.model.element.TypeElement;
import javax.lang.model.type.DeclaredType;
import javax.lang.model.type.TypeKind;
import javax.lang.model.type.TypeMirror;

/** One {@code @Provides} method: the binding for its return type, backed by a generated factory. */
final class ProvidesBinding {
  final TypeElement module;
  final ExecutableElement method;
  final boolean singleton;

  ProvidesBinding(TypeElement module, ExecutableElement method, boolean singleton) {
    this.module = module;
    this.method = method;
    this.singleton = singleton;
  }

  boolean isStatic() {
    return method.getModifiers().contains(Modifier.STATIC);
  }

  TypeMirror returnType() {
    return method.getReturnType();
  }

  /** The provided type's element, or null if the return type is not a declared type. */
  TypeElement returnElement() {
    TypeMirror rt = method.getReturnType();
    if (rt.getKind() != TypeKind.DECLARED) {
      return null;
    }
    return (TypeElement) ((DeclaredType) rt).asElement();
  }

  String factorySimpleName() {
    return Names.providesFactorySimpleName(module, method);
  }

  String factoryQualifiedName() {
    return Names.providesFactoryQualifiedName(module, method);
  }

  /** {@code pkg.Mod.provideFoo} for diagnostics. */
  String describe() {
    return Names.ref(module) + "." + method.getSimpleName() + "()";
  }
}
