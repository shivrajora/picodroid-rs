// SPDX-License-Identifier: GPL-3.0-only
package picodroid.inject.compiler;

import java.util.List;
import javax.lang.model.element.TypeElement;

/**
 * A {@code @Module} class and its {@code @Provides} methods. Every module in the compilation is
 * installed into the single implicit component; a module with instance methods is instantiated
 * once, lazily, through a generated {@code Mod_Factory}.
 */
final class ModuleInfo {
  final TypeElement type;
  final List<ProvidesBinding> provides;

  ModuleInfo(TypeElement type, List<ProvidesBinding> provides) {
    this.type = type;
    this.provides = provides;
  }

  String qualifiedName() {
    return type.getQualifiedName().toString();
  }

  /** True if any {@code @Provides} method is an instance method. */
  boolean needsInstance() {
    for (ProvidesBinding pb : provides) {
      if (!pb.isStatic()) {
        return true;
      }
    }
    return false;
  }
}
