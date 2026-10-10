// SPDX-License-Identifier: GPL-3.0-only
package java.io;

/**
 * Mirrors {@code java.io.Writer}: the abstract character sink behind {@link OutputStreamWriter} and
 * {@link PrintWriter}. A subclass implements {@link #write(char[], int, int)}, {@link #flush()} and
 * {@link #close()}. Chars are written as bytes, one each (see {@link Reader}).
 */
public abstract class Writer implements Closeable {
  protected Writer() {}

  public void write(int c) throws IOException {
    char[] one = new char[1];
    one[0] = (char) c;
    write(one, 0, 1);
  }

  public void write(char[] cbuf) throws IOException {
    write(cbuf, 0, cbuf.length);
  }

  public abstract void write(char[] cbuf, int off, int len) throws IOException;

  public void write(String str) throws IOException {
    write(str.toCharArray(), 0, str.length());
  }

  public void write(String str, int off, int len) throws IOException {
    write(str.substring(off, off + len));
  }

  public Writer append(CharSequence csq) throws IOException {
    write(csq == null ? "null" : csq.toString());
    return this;
  }

  public Writer append(char c) throws IOException {
    write(c);
    return this;
  }

  public abstract void flush() throws IOException;

  @Override
  public abstract void close() throws IOException;
}
