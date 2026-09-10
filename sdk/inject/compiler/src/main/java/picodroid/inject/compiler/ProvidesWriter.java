// SPDX-License-Identifier: GPL-3.0-only
package picodroid.inject.compiler;

import java.io.IOException;
import javax.annotation.processing.ProcessingEnvironment;
import javax.lang.model.element.TypeElement;
import javax.lang.model.element.VariableElement;

/**
 * Emits {@code Mod_ProvideFooFactory} for every {@code @Provides} method — {@code get()} calls the
 * method (statically, or on the module's singleton instance) with its dependencies resolved like
 * constructor parameters; {@code @Singleton} methods get the same double-checked holder as
 * singleton classes — and {@code Mod_Factory}, the lazy singleton holder for a module with instance
 * methods.
 */
final class ProvidesWriter {
  private ProvidesWriter() {}

  static void write(ProcessingEnvironment env, InjectionGraph graph, ProvidesBinding pb)
      throws IOException {
    TypeElement module = pb.module;
    TypeElement returned = pb.returnElement();
    String simple = pb.factorySimpleName();
    String ref = Names.ref(returned);

    StringBuilder args = new StringBuilder();
    for (VariableElement param : pb.method.getParameters()) {
      if (args.length() > 0) {
        args.append(", ");
      }
      args.append(SourceWriter.dependencyExpr(graph, param.asType()));
    }
    String receiver =
        pb.isStatic()
            ? Names.ref(module)
            : Names.generatedQualifiedName(module, Names.FACTORY_SUFFIX) + ".get()";
    String call = receiver + "." + pb.method.getSimpleName() + "(" + args + ")";

    StringBuilder sb = SourceWriter.begin(module);
    sb.append("public final class ").append(simple).append(" {\n");
    if (pb.singleton) {
      sb.append("  private static ").append(ref).append(" instance;\n\n");
    }
    sb.append("  private ").append(simple).append("() {}\n\n");
    sb.append("  public static ").append(ref).append(" get() {\n");
    if (pb.singleton) {
      appendSingletonBody(sb, ref, simple, call);
    } else {
      sb.append("    return ").append(call).append(";\n");
    }
    sb.append("  }\n");
    sb.append("}\n");
    SourceWriter.emit(env, pb.factoryQualifiedName(), sb, module);
  }

  /** {@code Mod_Factory}: one lazily-created module instance for its instance methods. */
  static void writeModuleFactory(ProcessingEnvironment env, TypeElement module) throws IOException {
    String simple = Names.generatedSimpleName(module, Names.FACTORY_SUFFIX);
    String ref = Names.ref(module);
    StringBuilder sb = SourceWriter.begin(module);
    sb.append("public final class ").append(simple).append(" {\n");
    sb.append("  private static ").append(ref).append(" instance;\n\n");
    sb.append("  private ").append(simple).append("() {}\n\n");
    sb.append("  public static ").append(ref).append(" get() {\n");
    appendSingletonBody(sb, ref, simple, "new " + ref + "()");
    sb.append("  }\n");
    sb.append("}\n");
    SourceWriter.emit(env, Names.generatedQualifiedName(module, Names.FACTORY_SUFFIX), sb, module);
  }

  private static void appendSingletonBody(
      StringBuilder sb, String ref, String simple, String construct) {
    sb.append("    ").append(ref).append(" local = instance;\n");
    sb.append("    if (local == null) {\n");
    sb.append("      synchronized (").append(simple).append(".class) {\n");
    sb.append("        local = instance;\n");
    sb.append("        if (local == null) {\n");
    sb.append("          local = ").append(construct).append(";\n");
    sb.append("          instance = local;\n");
    sb.append("        }\n");
    sb.append("      }\n");
    sb.append("    }\n");
    sb.append("    return local;\n");
  }
}
