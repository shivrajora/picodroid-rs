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

  public native boolean exists();

  public native boolean isFile();

  public native boolean isDirectory();

  public native long length();

  public native boolean delete();

  public native boolean mkdir();

  public native boolean renameTo(File dest);
}
