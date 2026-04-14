package picodroid.io;

public class FileOutputStream implements AutoCloseable {
  private String path;
  private long pos;

  public FileOutputStream(File f) {
    this(f.getPath(), false);
  }

  public FileOutputStream(String path) {
    this(path, false);
  }

  public FileOutputStream(String path, boolean append) {
    this.path = path;
    this.pos = initStream(path, append);
  }

  // Truncates when append=false, returns current file size when append=true.
  private static native long initStream(String path, boolean append);

  public native void write(byte[] buf, int off, int len);

  public void write(byte[] buf) {
    write(buf, 0, buf.length);
  }

  public void write(int b) {
    byte[] one = new byte[1];
    one[0] = (byte) b;
    write(one, 0, 1);
  }

  public native void flush();

  @Override
  public void close() {
    // No native handle to release — each write() is standalone.
  }
}
