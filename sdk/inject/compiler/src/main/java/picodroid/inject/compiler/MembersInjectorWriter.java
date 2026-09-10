// SPDX-License-Identifier: GPL-3.0-only
package picodroid.inject.compiler;

import java.io.IOException;
import javax.annotation.processing.ProcessingEnvironment;
import javax.lang.model.element.ExecutableElement;
import javax.lang.model.element.TypeElement;
import javax.lang.model.element.VariableElement;

/**
 * Emits {@code Foo_MembersInjector.injectMembers(Foo)}: the nearest injectable superclass first
 * (Dagger order), then this class's {@code @Inject} fields, then its {@code @Inject} methods, each
 * in declaration order. Stateless by design — the runtime calls it through {@code
 * Jvm::invoke_static_with_args}, which does not run {@code <clinit>} for the entry class.
 */
final class MembersInjectorWriter {
  private MembersInjectorWriter() {}

  static void write(ProcessingEnvironment env, InjectionGraph graph, Binding b) throws IOException {
    TypeElement type = b.type;
    String simple = Names.generatedSimpleName(type, Names.MEMBERS_INJECTOR_SUFFIX);
    String ref = Names.ref(type);

    StringBuilder sb = SourceWriter.begin(type);
    sb.append("public final class ").append(simple).append(" {\n");
    sb.append("  private ").append(simple).append("() {}\n\n");
    sb.append("  public static void injectMembers(").append(ref).append(" instance) {\n");
    if (b.nearestInjectableAncestor != null) {
      sb.append("    ")
          .append(SourceWriter.injectorCall(b.nearestInjectableAncestor.type, "instance"))
          .append(";\n");
    }
    for (VariableElement field : b.injectFields) {
      sb.append("    instance.")
          .append(field.getSimpleName())
          .append(" = ")
          .append(SourceWriter.dependencyExpr(graph, field.asType()))
          .append(";\n");
    }
    for (ExecutableElement method : b.injectMethods) {
      sb.append("    instance.").append(method.getSimpleName()).append('(');
      boolean first = true;
      for (VariableElement param : method.getParameters()) {
        if (!first) {
          sb.append(", ");
        }
        first = false;
        sb.append(SourceWriter.dependencyExpr(graph, param.asType()));
      }
      sb.append(");\n");
    }
    sb.append("  }\n");
    sb.append("}\n");
    SourceWriter.emit(
        env, Names.generatedQualifiedName(type, Names.MEMBERS_INJECTOR_SUFFIX), sb, type);
  }
}
