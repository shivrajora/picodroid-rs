// SPDX-License-Identifier: GPL-3.0-only
package picodroid.inject.compiler;

import java.util.ArrayList;
import java.util.Collection;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;
import javax.annotation.processing.ProcessingEnvironment;
import javax.annotation.processing.RoundEnvironment;
import javax.lang.model.element.AnnotationMirror;
import javax.lang.model.element.Element;
import javax.lang.model.element.ExecutableElement;
import javax.lang.model.element.TypeElement;
import javax.lang.model.element.VariableElement;
import javax.lang.model.type.DeclaredType;
import javax.lang.model.type.TypeKind;
import javax.lang.model.type.TypeMirror;
import javax.lang.model.util.Elements;
import javax.lang.model.util.Types;

/**
 * The whole-compilation view: one {@link Binding} per class that takes part in injection. Built
 * once from the round's root elements (nested types included) — the processor is aggregating
 * because cycle detection, dependency resolution and the field-shadowing check all need every class
 * at once.
 */
final class InjectionGraph {
  private static final String[] FRAMEWORK_COMPONENTS = {
    "picodroid.app.Application", "picodroid.app.Activity", "picodroid.app.Service",
  };

  private final Types types;
  private final Elements elements;
  private final Map<String, Binding> bindings = new LinkedHashMap<>();
  private final List<TypeElement> allTypes = new ArrayList<>();
  private final List<TypeMirror> frameworkTypes = new ArrayList<>();

  private InjectionGraph(ProcessingEnvironment env) {
    this.types = env.getTypeUtils();
    this.elements = env.getElementUtils();
    for (String name : FRAMEWORK_COMPONENTS) {
      TypeElement t = elements.getTypeElement(name);
      if (t != null) {
        frameworkTypes.add(types.erasure(t.asType()));
      }
    }
  }

  static InjectionGraph collect(ProcessingEnvironment env, RoundEnvironment round) {
    InjectionGraph g = new InjectionGraph(env);
    for (Element root : round.getRootElements()) {
      g.collectTypes(root);
    }
    List<TypeElement> unannotatedComponents = new ArrayList<>();
    for (TypeElement type : g.allTypes) {
      if (!g.addBinding(type) && g.isFrameworkComponent(type)) {
        unannotatedComponents.add(type);
      }
    }
    for (Binding b : g.bindings.values()) {
      b.nearestInjectableAncestor = g.findInjectableAncestor(b.type);
    }
    // Concrete framework components that only inherit @Inject members still
    // get a leaf injector, so the runtime never has to walk the superclass
    // chain itself.
    for (TypeElement type : unannotatedComponents) {
      Binding ancestor = g.findInjectableAncestor(type);
      if (ancestor != null) {
        Binding leaf =
            new Binding(
                type,
                new ArrayList<ExecutableElement>(),
                new ArrayList<VariableElement>(),
                new ArrayList<ExecutableElement>(),
                false,
                new ArrayList<AnnotationMirror>(),
                new ArrayList<Element>(),
                true);
        leaf.nearestInjectableAncestor = ancestor;
        g.bindings.put(leaf.qualifiedName(), leaf);
      }
    }
    return g;
  }

  private void collectTypes(Element e) {
    if (e instanceof TypeElement) {
      allTypes.add((TypeElement) e);
    }
    for (Element child : e.getEnclosedElements()) {
      if (child.getKind().isClass() || child.getKind().isInterface()) {
        collectTypes(child);
      }
    }
  }

  /**
   * Registers a binding if {@code type} carries any inject/scope annotation; returns whether it
   * did.
   */
  private boolean addBinding(TypeElement type) {
    List<ExecutableElement> ctors = new ArrayList<>();
    List<VariableElement> fields = new ArrayList<>();
    List<ExecutableElement> methods = new ArrayList<>();
    List<Element> misplacedSingletons = new ArrayList<>();
    for (Element member : type.getEnclosedElements()) {
      if (Names.hasAnnotation(member, Names.SINGLETON)) {
        misplacedSingletons.add(member);
      }
      if (!Names.hasAnnotation(member, Names.INJECT)) {
        continue;
      }
      switch (member.getKind()) {
        case CONSTRUCTOR:
          ctors.add((ExecutableElement) member);
          break;
        case FIELD:
          fields.add((VariableElement) member);
          break;
        case METHOD:
          methods.add((ExecutableElement) member);
          break;
        default:
          break;
      }
    }
    boolean singleton = false;
    List<AnnotationMirror> foreignScopes = new ArrayList<>();
    for (AnnotationMirror am : type.getAnnotationMirrors()) {
      // @Singleton is matched by name: its own @Scope meta-annotation is
      // SOURCE-retained and therefore invisible once Singleton arrives as a
      // class file. Custom scopes declared in this compilation are visible.
      if (Names.qualifiedNameOf(am).equals(Names.SINGLETON)) {
        singleton = true;
      } else if (Names.hasAnnotation(am.getAnnotationType().asElement(), Names.SCOPE)) {
        foreignScopes.add(am);
      }
    }
    if (ctors.isEmpty()
        && fields.isEmpty()
        && methods.isEmpty()
        && !singleton
        && foreignScopes.isEmpty()
        && misplacedSingletons.isEmpty()) {
      return false;
    }
    Binding b =
        new Binding(
            type,
            ctors,
            fields,
            methods,
            singleton,
            foreignScopes,
            misplacedSingletons,
            isFrameworkComponent(type));
    bindings.put(b.qualifiedName(), b);
    return true;
  }

  private Binding findInjectableAncestor(TypeElement type) {
    TypeMirror sup = type.getSuperclass();
    while (sup.getKind() == TypeKind.DECLARED) {
      TypeElement st = (TypeElement) ((DeclaredType) sup).asElement();
      Binding b = bindings.get(st.getQualifiedName().toString());
      if (b != null && b.hasOwnMembers()) {
        return b;
      }
      sup = st.getSuperclass();
    }
    return null;
  }

  boolean isFrameworkComponent(TypeElement type) {
    TypeMirror erased = types.erasure(type.asType());
    for (TypeMirror fw : frameworkTypes) {
      if (types.isSubtype(erased, fw)) {
        return true;
      }
    }
    return false;
  }

  Collection<Binding> bindings() {
    return bindings.values();
  }

  Binding binding(TypeElement type) {
    return bindings.get(type.getQualifiedName().toString());
  }

  /** Every class, interface and enum in the compilation (used by the shadowing check). */
  List<TypeElement> allTypes() {
    return allTypes;
  }

  Types types() {
    return types;
  }

  Elements elements() {
    return elements;
  }
}
