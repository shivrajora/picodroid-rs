// SPDX-License-Identifier: GPL-3.0-only
package picodroid.inject.compiler;

import java.io.IOException;
import java.util.Set;
import javax.annotation.processing.AbstractProcessor;
import javax.annotation.processing.RoundEnvironment;
import javax.annotation.processing.SupportedAnnotationTypes;
import javax.lang.model.SourceVersion;
import javax.lang.model.element.TypeElement;
import javax.tools.Diagnostic;

/**
 * Compile-time DI for picodroid apps: {@code @Inject} + {@code @Singleton}, Dagger-shaped output,
 * nothing at runtime.
 *
 * <p>Runs once per compilation (every user source is in the first round and the generated sources
 * carry no annotations): collect the {@link InjectionGraph}, {@link Validator validate} it, then
 * emit a {@code Foo_Factory} per {@code @Inject} constructor and a {@code Foo_MembersInjector} per
 * class with {@code @Inject} members. The generated classes are compiled with the app and packed
 * into its PAPK like any other class; the runtime hook in {@code picodroid-core/src/lifecycle.rs}
 * calls {@code <Component>_MembersInjector.injectMembers} on every Application / Activity / Service
 * it instantiates.
 *
 * <p>See {@code docs/designs/inject-annotations-2026-08.md} for the contract and its rationale.
 */
@SupportedAnnotationTypes({"javax.inject.Inject", "javax.inject.Singleton"})
public final class InjectProcessor extends AbstractProcessor {
  private boolean done;

  @Override
  public SourceVersion getSupportedSourceVersion() {
    return SourceVersion.latestSupported();
  }

  @Override
  public boolean process(Set<? extends TypeElement> annotations, RoundEnvironment env) {
    if (done || env.processingOver()) {
      return false;
    }
    done = true;
    InjectionGraph graph = InjectionGraph.collect(processingEnv, env);
    if (!new Validator(processingEnv, graph).validate()) {
      return false;
    }
    for (Binding b : graph.bindings()) {
      try {
        if (b.hasInjectConstructor()) {
          FactoryWriter.write(processingEnv, b);
        }
        if (b.needsMembersInjector()) {
          MembersInjectorWriter.write(processingEnv, b);
        }
      } catch (IOException e) {
        processingEnv
            .getMessager()
            .printMessage(
                Diagnostic.Kind.ERROR,
                "Could not write generated source for " + b.qualifiedName() + ": " + e,
                b.type);
      }
    }
    // Never claim javax.inject.*: other tools may want to see them.
    return false;
  }
}
