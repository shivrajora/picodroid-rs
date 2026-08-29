// SPDX-License-Identifier: GPL-3.0-only
package picodroid.inject.compiler;

import java.io.IOException;
import javax.annotation.processing.ProcessingEnvironment;
import javax.lang.model.element.TypeElement;
import javax.lang.model.element.VariableElement;

/**
 * Emits {@code Foo_Factory} for a class with an {@code @Inject} constructor.
 *
 * <p>Unscoped: {@code get()} constructs a fresh instance and runs member injection.
 * {@code @Singleton}: double-checked locking on a static field. pico-jvm honours {@code
 * synchronized} blocks ({@code monitorenter}) but not {@code ACC_SYNCHRONIZED} methods, class
 * literals are canonical objects, and statics live on the shared heap as GC roots — so the classic
 * DCL shape is both correct and the cheapest option. Members are injected before the instance is
 * published.
 */
final class FactoryWriter {
  private FactoryWriter() {}

  static void write(ProcessingEnvironment env, InjectionGraph graph, Binding b) throws IOException {
    TypeElement type = b.type;
    String simple = Names.generatedSimpleName(type, Names.FACTORY_SUFFIX);
    String ref = Names.ref(type);
    TypeElement injectorOwner = b.membersInjectorOwner();

    StringBuilder args = new StringBuilder();
    for (VariableElement param : b.injectConstructor().getParameters()) {
      if (args.length() > 0) {
        args.append(", ");
      }
      args.append(SourceWriter.dependencyExpr(graph, param.asType()));
    }
    String construct = "new " + ref + "(" + args + ")";

    StringBuilder sb = SourceWriter.begin(type);
    sb.append("public final class ").append(simple).append(" {\n");
    if (b.singleton) {
      sb.append("  private static ").append(ref).append(" instance;\n\n");
    }
    sb.append("  private ").append(simple).append("() {}\n\n");
    sb.append("  public static ").append(ref).append(" get() {\n");
    if (b.singleton) {
      sb.append("    ").append(ref).append(" local = instance;\n");
      sb.append("    if (local == null) {\n");
      sb.append("      synchronized (").append(simple).append(".class) {\n");
      sb.append("        local = instance;\n");
      sb.append("        if (local == null) {\n");
      sb.append("          local = ").append(construct).append(";\n");
      if (injectorOwner != null) {
        sb.append("          ")
            .append(SourceWriter.injectorCall(injectorOwner, "local"))
            .append(";\n");
      }
      sb.append("          instance = local;\n");
      sb.append("        }\n");
      sb.append("      }\n");
      sb.append("    }\n");
      sb.append("    return local;\n");
    } else if (injectorOwner == null) {
      sb.append("    return ").append(construct).append(";\n");
    } else {
      sb.append("    ").append(ref).append(" instance = ").append(construct).append(";\n");
      sb.append("    ").append(SourceWriter.injectorCall(injectorOwner, "instance")).append(";\n");
      sb.append("    return instance;\n");
    }
    sb.append("  }\n");
    sb.append("}\n");
    SourceWriter.emit(env, Names.generatedQualifiedName(type, Names.FACTORY_SUFFIX), sb, type);
  }
}
