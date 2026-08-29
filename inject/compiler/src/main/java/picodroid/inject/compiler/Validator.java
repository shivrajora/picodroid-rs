// SPDX-License-Identifier: GPL-3.0-only
package picodroid.inject.compiler;

import java.util.ArrayList;
import java.util.HashMap;
import java.util.List;
import java.util.Map;
import javax.annotation.processing.Messager;
import javax.annotation.processing.ProcessingEnvironment;
import javax.lang.model.element.AnnotationMirror;
import javax.lang.model.element.Element;
import javax.lang.model.element.ElementKind;
import javax.lang.model.element.ExecutableElement;
import javax.lang.model.element.Modifier;
import javax.lang.model.element.NestingKind;
import javax.lang.model.element.TypeElement;
import javax.lang.model.element.VariableElement;
import javax.lang.model.type.DeclaredType;
import javax.lang.model.type.TypeKind;
import javax.lang.model.type.TypeMirror;
import javax.lang.model.util.Types;
import javax.tools.Diagnostic;

/**
 * Compile-time rules. Dagger's JSR-330 rules plus the ones pico-jvm forces: framework components
 * keep a no-arg constructor (the runtime invokes {@code <init>} by name and pads missing arguments
 * with null), instance fields resolve by name only (so shadowing is an error), and there is no
 * {@code Provider}/{@code @Provides} yet (so every dependency must be a concrete class with an
 * {@code @Inject} constructor).
 */
final class Validator {
  private static final String DESIGN_DOC = "docs/designs/inject-annotations-2026-08.md";

  private final InjectionGraph graph;
  private final Messager messager;
  private final Types types;
  private boolean ok = true;

  Validator(ProcessingEnvironment env, InjectionGraph graph) {
    this.graph = graph;
    this.messager = env.getMessager();
    this.types = graph.types();
  }

  /** Reports every problem it finds; returns false if anything was reported. */
  boolean validate() {
    for (Binding b : graph.bindings()) {
      validateBinding(b);
    }
    checkShadowing();
    if (ok) {
      checkCycles();
    }
    return ok;
  }

  private void error(Element at, String message) {
    ok = false;
    messager.printMessage(Diagnostic.Kind.ERROR, message, at);
  }

  private void error(Element at, AnnotationMirror am, String message) {
    ok = false;
    messager.printMessage(Diagnostic.Kind.ERROR, message, at, am);
  }

  private void validateBinding(Binding b) {
    TypeElement type = b.type;
    String name = Names.ref(type);
    if (type.getKind() != ElementKind.CLASS) {
      error(type, "@Inject and @Singleton can only be used on classes; " + name + " is not one.");
      return;
    }
    NestingKind nesting = type.getNestingKind();
    if (nesting == NestingKind.LOCAL || nesting == NestingKind.ANONYMOUS) {
      error(type, "@Inject is not supported in local or anonymous classes (" + name + ").");
      return;
    }
    if (nesting == NestingKind.MEMBER && !type.getModifiers().contains(Modifier.STATIC)) {
      error(type, "Nested classes using @Inject must be static (" + name + " is an inner class).");
      return;
    }
    if (!type.getTypeParameters().isEmpty()) {
      error(type, "Generic classes are not supported by @Inject (" + name + ").");
      return;
    }
    for (Element member : b.misplacedSingletons) {
      error(
          member,
          "@Singleton is only supported on classes with an @Inject constructor; @Provides methods"
              + " are not supported yet ("
              + name
              + "."
              + member.getSimpleName()
              + ").");
    }
    for (AnnotationMirror scope : b.foreignScopes) {
      error(
          type,
          scope,
          "Only @Singleton is supported as a scope; @"
              + scope.getAnnotationType().asElement().getSimpleName()
              + " on "
              + name
              + " is not.");
    }

    if (b.injectConstructors.size() > 1) {
      for (ExecutableElement ctor : b.injectConstructors.subList(1, b.injectConstructors.size())) {
        error(ctor, "Types may only contain one @Inject constructor (" + name + ").");
      }
    }
    if (b.hasInjectConstructor()) {
      ExecutableElement ctor = b.injectConstructor();
      if (b.frameworkComponent) {
        error(
            ctor,
            name
                + " is instantiated by the framework and must keep a no-arg constructor;"
                + " use @Inject on fields or methods instead.");
      } else if (b.isAbstract()) {
        error(ctor, "Abstract classes cannot have an @Inject constructor (" + name + ").");
      } else if (ctor.getModifiers().contains(Modifier.PRIVATE)) {
        error(ctor, "@Inject constructors must not be private (" + name + ").");
      }
      for (VariableElement param : ctor.getParameters()) {
        checkDependency(param.asType(), param);
      }
    } else if (b.singleton) {
      error(type, "@Singleton requires an @Inject constructor (" + name + ").");
    }

    for (VariableElement field : b.injectFields) {
      String fieldName = name + "." + field.getSimpleName();
      if (field.getModifiers().contains(Modifier.PRIVATE)) {
        error(
            field,
            "@Inject fields must not be private; the generated injector lives in the same package ("
                + fieldName
                + ").");
      } else if (field.getModifiers().contains(Modifier.FINAL)) {
        error(field, "@Inject fields must not be final (" + fieldName + ").");
      } else if (field.getModifiers().contains(Modifier.STATIC)) {
        error(field, "@Inject fields must not be static (" + fieldName + ").");
      }
      checkDependency(field.asType(), field);
    }

    for (ExecutableElement method : b.injectMethods) {
      String methodName = name + "." + method.getSimpleName();
      if (method.getModifiers().contains(Modifier.PRIVATE)) {
        error(method, "@Inject methods must not be private (" + methodName + ").");
      } else if (method.getModifiers().contains(Modifier.STATIC)) {
        error(method, "@Inject methods must not be static (" + methodName + ").");
      } else if (method.getModifiers().contains(Modifier.ABSTRACT)) {
        error(method, "@Inject methods must not be abstract (" + methodName + ").");
      } else if (!method.getTypeParameters().isEmpty()) {
        error(method, "@Inject methods must not be generic (" + methodName + ").");
      }
      for (VariableElement param : method.getParameters()) {
        checkDependency(param.asType(), param);
      }
    }
  }

  /** A dependency must be a concrete, non-generic class with an @Inject constructor. */
  private void checkDependency(TypeMirror t, Element site) {
    switch (t.getKind()) {
      case ERROR:
        // javac already reported the unresolved symbol; just don't generate.
        ok = false;
        return;
      case DECLARED:
        break;
      case ARRAY:
        error(site, "Arrays cannot be injected (" + t + ").");
        return;
      case TYPEVAR:
      case WILDCARD:
        error(site, "Type variables cannot be injected (" + t + ").");
        return;
      default:
        error(site, "Primitive types cannot be injected (" + t + ").");
        return;
    }
    DeclaredType dt = (DeclaredType) t;
    TypeElement te = (TypeElement) dt.asElement();
    String name = Names.ref(te);
    if (!dt.getTypeArguments().isEmpty()) {
      error(
          site,
          "Parameterized types cannot be injected ("
              + t
              + "); Provider<T>/Lazy<T> are not supported yet — see "
              + DESIGN_DOC
              + ".");
      return;
    }
    if (te.getKind() != ElementKind.CLASS || te.getModifiers().contains(Modifier.ABSTRACT)) {
      error(
          site,
          name
              + " is "
              + (te.getKind() == ElementKind.CLASS ? "abstract" : "not a class")
              + " and cannot be provided without an @Inject constructor;"
              + " @Provides/@Binds are not supported yet — see "
              + DESIGN_DOC
              + ".");
      return;
    }
    Binding provider = graph.binding(te);
    if (provider == null || !provider.hasInjectConstructor()) {
      error(site, name + " cannot be provided without an @Inject constructor.");
    }
  }

  /**
   * pico-jvm resolves instance fields by name on the runtime class, ignoring the declaring class,
   * so an {@code @Inject} field whose name is reused anywhere up or down its class chain would be
   * written into the wrong slot.
   */
  private void checkShadowing() {
    for (Binding b : graph.bindings()) {
      for (VariableElement field : b.injectFields) {
        String fieldName = field.getSimpleName().toString();
        TypeMirror sup = b.type.getSuperclass();
        while (sup.getKind() == TypeKind.DECLARED) {
          TypeElement st = (TypeElement) ((DeclaredType) sup).asElement();
          reportShadow(b, field, fieldName, st);
          sup = st.getSuperclass();
        }
        TypeMirror self = types.erasure(b.type.asType());
        for (TypeElement other : graph.allTypes()) {
          if (!other.equals(b.type) && types.isSubtype(types.erasure(other.asType()), self)) {
            reportShadow(b, field, fieldName, other);
          }
        }
      }
    }
  }

  private void reportShadow(Binding b, VariableElement field, String fieldName, TypeElement other) {
    for (Element member : other.getEnclosedElements()) {
      if (member.getKind() == ElementKind.FIELD
          && !member.getModifiers().contains(Modifier.STATIC)
          && member.getSimpleName().contentEquals(fieldName)) {
        error(
            field,
            "pico-jvm resolves instance fields by name only: @Inject field '"
                + fieldName
                + "' in "
                + Names.ref(b.type)
                + " collides with field '"
                + fieldName
                + "' declared in "
                + Names.ref(other)
                + "; rename one of them.");
      }
    }
  }

  // ── Cycle detection ────────────────────────────────────────────────────────

  private static final int WHITE = 0;
  private static final int GREY = 1;
  private static final int BLACK = 2;

  private void checkCycles() {
    Map<String, Integer> color = new HashMap<>();
    for (Binding b : graph.bindings()) {
      if (color.getOrDefault(b.qualifiedName(), WHITE) == WHITE) {
        List<Binding> path = new ArrayList<>();
        if (dfs(b, color, path)) {
          return;
        }
      }
    }
  }

  /** Returns true once a cycle has been reported (one is enough). */
  private boolean dfs(Binding b, Map<String, Integer> color, List<Binding> path) {
    color.put(b.qualifiedName(), GREY);
    path.add(b);
    for (Binding dep : dependencies(b)) {
      int c = color.getOrDefault(dep.qualifiedName(), WHITE);
      if (c == GREY) {
        reportCycle(path, dep);
        return true;
      }
      if (c == WHITE && dfs(dep, color, path)) {
        return true;
      }
    }
    path.remove(path.size() - 1);
    color.put(b.qualifiedName(), BLACK);
    return false;
  }

  /** Constructor parameters plus every member injected on construction (own and inherited). */
  private List<Binding> dependencies(Binding b) {
    List<Binding> deps = new ArrayList<>();
    if (b.hasInjectConstructor()) {
      for (VariableElement p : b.injectConstructor().getParameters()) {
        addDependency(deps, p.asType());
      }
    }
    for (Binding m = b; m != null; m = m.nearestInjectableAncestor) {
      for (VariableElement f : m.injectFields) {
        addDependency(deps, f.asType());
      }
      for (ExecutableElement method : m.injectMethods) {
        for (VariableElement p : method.getParameters()) {
          addDependency(deps, p.asType());
        }
      }
    }
    return deps;
  }

  private void addDependency(List<Binding> deps, TypeMirror t) {
    if (t.getKind() == TypeKind.DECLARED) {
      Binding dep = graph.binding((TypeElement) ((DeclaredType) t).asElement());
      if (dep != null) {
        deps.add(dep);
      }
    }
  }

  private void reportCycle(List<Binding> path, Binding back) {
    int start = path.indexOf(back);
    StringBuilder sb = new StringBuilder("Found a dependency cycle: ");
    for (int i = start; i < path.size(); i++) {
      sb.append(path.get(i).type.getSimpleName()).append(" -> ");
    }
    sb.append(back.type.getSimpleName());
    error(back.type, sb.toString());
  }
}
