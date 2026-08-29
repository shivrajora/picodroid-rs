// SPDX-License-Identifier: GPL-3.0-only
package picodroid.inject.compiler;

import javax.lang.model.element.TypeElement;
import javax.lang.model.type.DeclaredType;
import javax.lang.model.type.TypeKind;
import javax.lang.model.type.TypeMirror;

/**
 * One injection request, decoded from the declared type at the site: a plain {@code T}, a {@code
 * javax.inject.Provider<T>}, or a {@code picodroid.di.Lazy<T>}. Only {@code T} itself must be
 * providable; the wrappers are generated on demand ({@code T_Provider} / {@code T_Lazy}) and never
 * construct anything at injection time, which is what lets them break cycles.
 */
final class Dependency {
  enum Kind {
    DIRECT,
    PROVIDER,
    LAZY
  }

  final Kind kind;

  /** The provided type: the declared type for DIRECT, the single type argument for wrappers. */
  final TypeMirror provided;

  /** True for a raw {@code Provider} / {@code Lazy} (no type argument) — always an error. */
  final boolean rawWrapper;

  private Dependency(Kind kind, TypeMirror provided, boolean rawWrapper) {
    this.kind = kind;
    this.provided = provided;
    this.rawWrapper = rawWrapper;
  }

  static Dependency of(TypeMirror declared) {
    if (declared.getKind() == TypeKind.DECLARED) {
      DeclaredType dt = (DeclaredType) declared;
      String name = ((TypeElement) dt.asElement()).getQualifiedName().toString();
      Kind kind = null;
      if (name.equals(Names.PROVIDER)) {
        kind = Kind.PROVIDER;
      } else if (name.equals(Names.LAZY)) {
        kind = Kind.LAZY;
      }
      if (kind != null) {
        if (dt.getTypeArguments().size() == 1) {
          return new Dependency(kind, dt.getTypeArguments().get(0), false);
        }
        return new Dependency(kind, declared, true);
      }
    }
    return new Dependency(Kind.DIRECT, declared, false);
  }

  boolean isWrapper() {
    return kind != Kind.DIRECT;
  }

  /** The {@link TypeElement} of the provided type; only valid after validation (DECLARED). */
  TypeElement providedElement() {
    return (TypeElement) ((DeclaredType) provided).asElement();
  }
}
