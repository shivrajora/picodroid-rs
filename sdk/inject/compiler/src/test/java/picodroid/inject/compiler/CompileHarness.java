// SPDX-License-Identifier: GPL-3.0-only
package picodroid.inject.compiler;

import java.io.IOException;
import java.net.URI;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.Path;
import java.nio.file.Paths;
import java.util.ArrayList;
import java.util.Arrays;
import java.util.List;
import java.util.Map;
import java.util.TreeMap;
import java.util.stream.Stream;
import javax.tools.Diagnostic;
import javax.tools.DiagnosticCollector;
import javax.tools.JavaCompiler;
import javax.tools.JavaFileObject;
import javax.tools.SimpleJavaFileObject;
import javax.tools.StandardJavaFileManager;
import javax.tools.ToolProvider;

/**
 * Runs the real javac in-process with {@link InjectProcessor} on a set of in-memory sources, the
 * way an app build does (`--release 8`, SDK + annotations on the classpath), and hands back the
 * diagnostics, the generated sources and the compiled classes directory.
 */
final class CompileHarness {
  static final class Source {
    final String qualifiedName;
    final String code;

    Source(String qualifiedName, String code) {
      this.qualifiedName = qualifiedName;
      this.code = code;
    }
  }

  static final class Result {
    final boolean success;
    final List<Diagnostic<? extends JavaFileObject>> diagnostics;
    final Map<String, String> generated;
    final Path classesDir;

    Result(
        boolean success,
        List<Diagnostic<? extends JavaFileObject>> diagnostics,
        Map<String, String> generated,
        Path classesDir) {
      this.success = success;
      this.diagnostics = diagnostics;
      this.generated = generated;
      this.classesDir = classesDir;
    }

    List<String> errors() {
      List<String> out = new ArrayList<>();
      for (Diagnostic<? extends JavaFileObject> d : diagnostics) {
        if (d.getKind() == Diagnostic.Kind.ERROR) {
          out.add(d.getMessage(null));
        }
      }
      return out;
    }

    String generated(String qualifiedName) {
      String src = generated.get(qualifiedName);
      if (src == null) {
        throw new AssertionError(
            "no generated source " + qualifiedName + "; have " + generated.keySet());
      }
      return src;
    }
  }

  private CompileHarness() {}

  static Source src(String qualifiedName, String... lines) {
    return new Source(qualifiedName, String.join("\n", lines) + "\n");
  }

  static Result compile(Source... sources) throws IOException {
    JavaCompiler javac = ToolProvider.getSystemJavaCompiler();
    DiagnosticCollector<JavaFileObject> diags = new DiagnosticCollector<>();
    Path root = Files.createTempDirectory("picodroid-inject-test");
    Path gen = Files.createDirectories(root.resolve("gen"));
    Path out = Files.createDirectories(root.resolve("classes"));
    String cp = System.getProperty("picodroid.inject.testClasspath");
    if (cp == null) {
      cp = System.getProperty("java.class.path");
    }
    List<String> options =
        Arrays.asList(
            "--release",
            "8",
            "-Xlint:-options",
            "-implicit:none",
            "-classpath",
            cp,
            "-processorpath",
            cp,
            "-processor",
            InjectProcessor.class.getName(),
            "-s",
            gen.toString(),
            "-d",
            out.toString());
    List<JavaFileObject> units = new ArrayList<>();
    for (Source s : sources) {
      units.add(new StringSource(s.qualifiedName, s.code));
    }
    boolean ok;
    try (StandardJavaFileManager fm =
        javac.getStandardFileManager(diags, null, StandardCharsets.UTF_8)) {
      ok = javac.getTask(null, fm, diags, options, null, units).call();
    }
    Map<String, String> generated = new TreeMap<>();
    try (Stream<Path> files = Files.walk(gen)) {
      for (Path p : (Iterable<Path>) files::iterator) {
        if (Files.isRegularFile(p) && p.toString().endsWith(".java")) {
          String rel = gen.relativize(p).toString();
          String name =
              rel.substring(0, rel.length() - ".java".length())
                  .replace(java.io.File.separatorChar, '.');
          generated.put(name, new String(Files.readAllBytes(p), StandardCharsets.UTF_8));
        }
      }
    }
    return new Result(ok, diags.getDiagnostics(), generated, out);
  }

  private static final class StringSource extends SimpleJavaFileObject {
    private final String code;

    StringSource(String qualifiedName, String code) {
      super(
          URI.create("string:///" + qualifiedName.replace('.', '/') + Kind.SOURCE.extension),
          Kind.SOURCE);
      this.code = code;
    }

    @Override
    public CharSequence getCharContent(boolean ignoreEncodingErrors) {
      return code;
    }
  }

  static Path path(String first) {
    return Paths.get(first);
  }
}
