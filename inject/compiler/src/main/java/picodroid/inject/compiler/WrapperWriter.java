// SPDX-License-Identifier: GPL-3.0-only
package picodroid.inject.compiler;

import java.io.IOException;
import javax.annotation.processing.ProcessingEnvironment;
import javax.lang.model.element.TypeElement;

/**
 * Emits {@code T_Provider} ({@code javax.inject.Provider<T>}: a stateless delegate to {@code
 * T_Factory.get()}) and {@code T_Lazy} ({@code picodroid.di.Lazy<T>}: one memoized instance per
 * wrapper object, double-checked on {@code this}). Both are instantiated with {@code new} at each
 * injection site, so neither has static state.
 */
final class WrapperWriter {
  private WrapperWriter() {}

  static void writeProvider(ProcessingEnvironment env, TypeElement type) throws IOException {
    String simple = Names.generatedSimpleName(type, Names.PROVIDER_SUFFIX);
    String ref = Names.ref(type);
    StringBuilder sb = SourceWriter.begin(type);
    sb.append("public final class ")
        .append(simple)
        .append(" implements ")
        .append(Names.PROVIDER)
        .append('<')
        .append(ref)
        .append("> {\n");
    sb.append("  public ").append(simple).append("() {}\n\n");
    sb.append("  @Override\n");
    sb.append("  public ").append(ref).append(" get() {\n");
    sb.append("    return ").append(SourceWriter.factoryCall(type)).append(";\n");
    sb.append("  }\n");
    sb.append("}\n");
    SourceWriter.emit(env, Names.generatedQualifiedName(type, Names.PROVIDER_SUFFIX), sb, type);
  }

  static void writeLazy(ProcessingEnvironment env, TypeElement type) throws IOException {
    String simple = Names.generatedSimpleName(type, Names.LAZY_SUFFIX);
    String ref = Names.ref(type);
    StringBuilder sb = SourceWriter.begin(type);
    sb.append("public final class ")
        .append(simple)
        .append(" implements ")
        .append(Names.LAZY)
        .append('<')
        .append(ref)
        .append("> {\n");
    sb.append("  private ").append(ref).append(" value;\n\n");
    sb.append("  public ").append(simple).append("() {}\n\n");
    sb.append("  @Override\n");
    sb.append("  public ").append(ref).append(" get() {\n");
    sb.append("    ").append(ref).append(" local = value;\n");
    sb.append("    if (local == null) {\n");
    sb.append("      synchronized (this) {\n");
    sb.append("        local = value;\n");
    sb.append("        if (local == null) {\n");
    sb.append("          local = ").append(SourceWriter.factoryCall(type)).append(";\n");
    sb.append("          value = local;\n");
    sb.append("        }\n");
    sb.append("      }\n");
    sb.append("    }\n");
    sb.append("    return local;\n");
    sb.append("  }\n");
    sb.append("}\n");
    SourceWriter.emit(env, Names.generatedQualifiedName(type, Names.LAZY_SUFFIX), sb, type);
  }
}
