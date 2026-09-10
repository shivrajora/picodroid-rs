// SPDX-License-Identifier: GPL-3.0-only
package picodroid.inject.compiler;

import java.util.ArrayList;
import java.util.HashMap;
import java.util.HashSet;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;
import java.util.Set;
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
    validateModules();
    checkDuplicateBindings();
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
    if (graph.module(type) != null) {
      // Modules contribute bindings; they are not injection targets.
      for (ExecutableElement ctor : b.injectConstructors) {
        error(ctor, "@Module classes cannot have an @Inject constructor (" + name + ").");
      }
      for (Element member : b.injectFields) {
        error(member, "@Module classes cannot have @Inject members (" + name + ").");
      }
      for (Element member : b.injectMethods) {
        error(member, "@Module classes cannot have @Inject members (" + name + ").");
      }
      if (b.singleton) {
        error(
            type,
            "@Singleton on a @Module has no effect; put it on the @Provides methods instead ("
                + name
                + ").");
      }
      for (Element member : b.misplacedSingletons) {
        error(
            member,
            "@Singleton on a method is only supported on @Provides methods of a @Module ("
                + name
                + "."
                + member.getSimpleName()
                + ").");
      }
      return;
    }
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
          "@Singleton on a method is only supported on @Provides methods of a @Module ("
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

  /**
   * A dependency is a concrete, non-generic class with an @Inject constructor, optionally wrapped
   * in a single {@code Provider<T>} / {@code Lazy<T>}.
   */
  private void checkDependency(TypeMirror declared, Element site) {
    Dependency d = Dependency.of(declared);
    if (d.rawWrapper) {
      error(site, "Raw " + declared + " cannot be injected; use " + declared + "<T>.");
      return;
    }
    if (d.isWrapper() && Dependency.of(d.provided).isWrapper()) {
      error(site, "Nested Provider/Lazy are not supported (" + declared + ").");
      return;
    }
    if (checkProvidable(d.provided, site) && d.isWrapper()) {
      graph.requestWrapper(d.kind, d.providedElement());
    }
  }

  /** Returns whether {@code t} is a concrete, non-generic class with an @Inject constructor. */
  private boolean checkProvidable(TypeMirror t, Element site) {
    switch (t.getKind()) {
      case ERROR:
        // javac already reported the unresolved symbol; just don't generate.
        ok = false;
        return false;
      case DECLARED:
        break;
      case ARRAY:
        error(site, "Arrays cannot be injected (" + t + ").");
        return false;
      case TYPEVAR:
      case WILDCARD:
        error(site, "Type variables cannot be injected (" + t + ").");
        return false;
      default:
        error(site, "Primitive types cannot be injected (" + t + ").");
        return false;
    }
    DeclaredType dt = (DeclaredType) t;
    TypeElement te = (TypeElement) dt.asElement();
    String name = Names.ref(te);
    if (!dt.getTypeArguments().isEmpty()) {
      error(
          site,
          "Parameterized types cannot be injected ("
              + t
              + "); only javax.inject.Provider<T> and picodroid.di.Lazy<T> are supported — see "
              + DESIGN_DOC
              + ".");
      return false;
    }
    if (!graph.providesFor(te).isEmpty()) {
      // Bound by a @Provides method (duplicates are reported separately).
      return true;
    }
    if (te.getKind() != ElementKind.CLASS || te.getModifiers().contains(Modifier.ABSTRACT)) {
      error(
          site,
          name
              + " is "
              + (te.getKind() == ElementKind.CLASS ? "abstract" : "not a class")
              + " and cannot be provided without an @Inject constructor or a @Provides method"
              + " (see "
              + DESIGN_DOC
              + ").");
      return false;
    }
    Binding provider = graph.binding(te);
    if (provider == null || !provider.hasInjectConstructor()) {
      error(
          site, name + " cannot be provided without an @Inject constructor or a @Provides method.");
      return false;
    }
    return true;
  }

  // ── Modules ────────────────────────────────────────────────────────────────

  private void validateModules() {
    for (ExecutableElement m : graph.strayProvides()) {
      error(
          m,
          "@Provides methods can only be present within a @Module class ("
              + Names.ref((TypeElement) m.getEnclosingElement())
              + "."
              + m.getSimpleName()
              + ").");
    }
    for (ModuleInfo mod : graph.modules()) {
      TypeElement type = mod.type;
      String name = Names.ref(type);
      if (type.getKind() != ElementKind.CLASS) {
        error(type, "@Module can only be used on classes (" + name + ").");
        continue;
      }
      NestingKind nesting = type.getNestingKind();
      if (nesting == NestingKind.LOCAL || nesting == NestingKind.ANONYMOUS) {
        error(type, "@Module is not supported on local or anonymous classes (" + name + ").");
        continue;
      }
      if (nesting == NestingKind.MEMBER && !type.getModifiers().contains(Modifier.STATIC)) {
        error(type, "Nested @Module classes must be static (" + name + ").");
        continue;
      }
      if (!type.getTypeParameters().isEmpty()) {
        error(type, "@Module classes cannot be generic (" + name + ").");
        continue;
      }
      Set<String> methodNames = new HashSet<>();
      for (ProvidesBinding pb : mod.provides) {
        ExecutableElement m = pb.method;
        String mname = name + "." + m.getSimpleName();
        if (!methodNames.add(m.getSimpleName().toString())) {
          error(
              m,
              "Cannot have more than one @Provides method with the same name in a @Module ("
                  + mname
                  + ").");
        }
        if (m.getModifiers().contains(Modifier.PRIVATE)) {
          error(m, "@Provides methods must not be private (" + mname + ").");
        } else if (m.getModifiers().contains(Modifier.ABSTRACT)) {
          error(m, "@Provides methods must not be abstract (" + mname + ").");
        } else if (!m.getTypeParameters().isEmpty()) {
          error(m, "@Provides methods must not be generic (" + mname + ").");
        }
        TypeMirror rt = m.getReturnType();
        switch (rt.getKind()) {
          case ERROR:
            ok = false;
            break;
          case DECLARED:
            if (!((DeclaredType) rt).getTypeArguments().isEmpty()) {
              error(m, "@Provides methods cannot return parameterized types (" + rt + ").");
            }
            break;
          case VOID:
            error(m, "@Provides methods must return a value (" + mname + ").");
            break;
          default:
            error(
                m,
                "@Provides methods must return a class or interface type ("
                    + mname
                    + " returns "
                    + rt
                    + ").");
            break;
        }
        for (VariableElement param : m.getParameters()) {
          checkDependency(param.asType(), param);
        }
      }
      if (mod.needsInstance()) {
        if (type.getModifiers().contains(Modifier.ABSTRACT)) {
          error(
              type,
              "A @Module with instance @Provides methods must be concrete; make the methods"
                  + " static or the class concrete ("
                  + name
                  + ").");
        } else if (!hasAccessibleNoArgConstructor(type)) {
          error(
              type,
              "A @Module with instance @Provides methods needs a non-private no-arg constructor ("
                  + name
                  + ").");
        }
      }
    }
  }

  private static boolean hasAccessibleNoArgConstructor(TypeElement type) {
    boolean any = false;
    for (Element member : type.getEnclosedElements()) {
      if (member.getKind() != ElementKind.CONSTRUCTOR) {
        continue;
      }
      any = true;
      ExecutableElement ctor = (ExecutableElement) member;
      if (ctor.getParameters().isEmpty() && !ctor.getModifiers().contains(Modifier.PRIVATE)) {
        return true;
      }
    }
    return !any; // no explicit constructor → the implicit default one
  }

  /** A type may have exactly one binding: one @Provides method or one @Inject constructor. */
  private void checkDuplicateBindings() {
    for (Map.Entry<String, List<ProvidesBinding>> e : graph.provisions().entrySet()) {
      List<ProvidesBinding> provides = e.getValue();
      TypeElement provided = provides.get(0).returnElement();
      Binding ctorBinding = graph.binding(provided);
      List<String> sites = new ArrayList<>();
      for (ProvidesBinding pb : provides) {
        sites.add(pb.describe());
      }
      if (ctorBinding != null && ctorBinding.hasInjectConstructor()) {
        sites.add("@Inject " + Names.ref(provided) + "(...)");
      }
      if (sites.size() < 2) {
        continue;
      }
      String message = Names.ref(provided) + " is bound multiple times: " + sites;
      for (ProvidesBinding pb : provides) {
        error(pb.method, message);
      }
      if (ctorBinding != null && ctorBinding.hasInjectConstructor()) {
        error(ctorBinding.injectConstructor(), message);
      }
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

  /** Every node: classes with bindings plus every @Provides-bound type, by qualified name. */
  private Map<String, TypeElement> cycleNodes() {
    Map<String, TypeElement> nodes = new LinkedHashMap<>();
    for (Binding b : graph.bindings()) {
      nodes.put(b.qualifiedName(), b.type);
    }
    for (List<ProvidesBinding> provides : graph.provisions().values()) {
      TypeElement t = provides.get(0).returnElement();
      nodes.put(t.getQualifiedName().toString(), t);
    }
    return nodes;
  }

  private void checkCycles() {
    Map<String, TypeElement> nodes = cycleNodes();
    Map<String, Integer> color = new HashMap<>();
    for (String key : nodes.keySet()) {
      if (color.getOrDefault(key, WHITE) == WHITE) {
        List<String> path = new ArrayList<>();
        if (dfs(key, nodes, color, path)) {
          return;
        }
      }
    }
  }

  /** Returns true once a cycle has been reported (one is enough). */
  private boolean dfs(
      String key, Map<String, TypeElement> nodes, Map<String, Integer> color, List<String> path) {
    color.put(key, GREY);
    path.add(key);
    for (String dep : dependencies(key, nodes)) {
      int c = color.getOrDefault(dep, WHITE);
      if (c == GREY) {
        reportCycle(path, dep, nodes);
        return true;
      }
      if (c == WHITE && dfs(dep, nodes, color, path)) {
        return true;
      }
    }
    path.remove(path.size() - 1);
    color.put(key, BLACK);
    return false;
  }

  /**
   * Direct (non-wrapper) dependencies of a node: a @Provides method's parameters, or a class's
   * constructor parameters plus every member injected on construction (own and inherited).
   */
  private List<String> dependencies(String key, Map<String, TypeElement> nodes) {
    List<String> deps = new ArrayList<>();
    TypeElement type = nodes.get(key);
    List<ProvidesBinding> provides = graph.providesFor(type);
    if (provides.size() == 1) {
      for (VariableElement p : provides.get(0).method.getParameters()) {
        addDependency(deps, p.asType(), nodes);
      }
      return deps;
    }
    Binding b = graph.binding(type);
    if (b == null) {
      return deps;
    }
    if (b.hasInjectConstructor()) {
      for (VariableElement p : b.injectConstructor().getParameters()) {
        addDependency(deps, p.asType(), nodes);
      }
    }
    for (Binding m = b; m != null; m = m.nearestInjectableAncestor) {
      for (VariableElement f : m.injectFields) {
        addDependency(deps, f.asType(), nodes);
      }
      for (ExecutableElement method : m.injectMethods) {
        for (VariableElement p : method.getParameters()) {
          addDependency(deps, p.asType(), nodes);
        }
      }
    }
    return deps;
  }

  private void addDependency(List<String> deps, TypeMirror t, Map<String, TypeElement> nodes) {
    // Provider<T> / Lazy<T> construct nothing at injection time, so they are
    // not edges — that is exactly how they break a cycle (Dagger semantics).
    if (Dependency.of(t).isWrapper()) {
      return;
    }
    if (t.getKind() == TypeKind.DECLARED) {
      String key = ((TypeElement) ((DeclaredType) t).asElement()).getQualifiedName().toString();
      if (nodes.containsKey(key)) {
        deps.add(key);
      }
    }
  }

  private void reportCycle(List<String> path, String back, Map<String, TypeElement> nodes) {
    int start = path.indexOf(back);
    StringBuilder sb = new StringBuilder("Found a dependency cycle: ");
    for (int i = start; i < path.size(); i++) {
      sb.append(nodes.get(path.get(i)).getSimpleName()).append(" -> ");
    }
    sb.append(nodes.get(back).getSimpleName());
    error(nodes.get(back), sb.toString());
  }
}
