// SPDX-License-Identifier: GPL-3.0-only
package picodroid.io;

public class File {
  private String path;

  public File(String path) {
    this.path = path;
  }

  public String getPath() {
    return path;
  }

  /** The path with a leading {@code /}; LittleFS has a single root, so no cwd is involved. */
  public String getAbsolutePath() {
    return path.startsWith("/") ? path : "/" + path;
  }

  /** The last path segment. */
  public String getName() {
    int i = path.lastIndexOf('/');
    return i < 0 ? path : path.substring(i + 1);
  }

  /**
   * Parent path, or {@code null} when there is none ({@code "x"}); the parent of {@code "/x"} is
   * {@code "/"}.
   */
  public String getParent() {
    int i = path.lastIndexOf('/');
    if (i < 0) {
      return null;
    }
    return i == 0 ? "/" : path.substring(0, i);
  }

  public File getParentFile() {
    String p = getParent();
    return p == null ? null : new File(p);
  }

  /**
   * Creates this directory and any missing ancestors. Android semantics: {@code true} only if a
   * directory was created; {@code false} when it already existed or a step failed.
   */
  public boolean mkdirs() {
    if (exists()) {
      return false;
    }
    int from = path.startsWith("/") ? 1 : 0;
    while (true) {
      int i = path.indexOf('/', from);
      if (i < 0) {
        break;
      }
      if (i > from) {
        File step = new File(path.substring(0, i));
        if (!step.isDirectory() && !step.mkdir()) {
          return false;
        }
      }
      from = i + 1;
    }
    return mkdir();
  }

  /** Creates an empty file; {@code false} when the path already exists. */
  public native boolean createNewFile();

  public native boolean exists();

  public native boolean isFile();

  public native boolean isDirectory();

  public native long length();

  public native boolean delete();

  public native boolean mkdir();

  public native boolean renameTo(File dest);
}
