// SPDX-License-Identifier: GPL-3.0-only
package java.io;

/**
 * Mirrors {@code java.io.PrintWriter}: formatted text over a {@link Writer} or an {@link
 * OutputStream} that never throws {@link IOException} — a failed write sets the flag {@link
 * #checkError()} reports, as in the JDK. {@code println} ends a line with {@code \n} and, with
 * {@code autoFlush}, flushes. The usual socket shape works unchanged:
 *
 * <pre>{@code
 * PrintWriter out = new PrintWriter(socket.getOutputStream(), true);
 * out.println("HELLO");
 * }</pre>
 */
public class PrintWriter extends Writer {
  protected Writer out;
  private final boolean autoFlush;
  private boolean trouble;

  public PrintWriter(Writer out) {
    this(out, false);
  }

  public PrintWriter(Writer out, boolean autoFlush) {
    this.out = out;
    this.autoFlush = autoFlush;
  }

  public PrintWriter(OutputStream out) {
    this(out, false);
  }

  public PrintWriter(OutputStream out, boolean autoFlush) {
    this(new OutputStreamWriter(out), autoFlush);
  }

  private void ensureOpen() throws IOException {
    if (out == null) {
      throw new IOException("Stream closed");
    }
  }

  @Override
  public void flush() {
    try {
      ensureOpen();
      out.flush();
    } catch (IOException e) {
      trouble = true;
    }
  }

  @Override
  public void close() {
    try {
      if (out == null) {
        return;
      }
      out.close();
      out = null;
    } catch (IOException e) {
      trouble = true;
    }
  }

  /** Whether any write since construction failed; flushes first, as the JDK does. */
  public boolean checkError() {
    if (out != null) {
      flush();
    }
    return trouble;
  }

  @Override
  public void write(int c) {
    try {
      ensureOpen();
      out.write(c);
    } catch (IOException e) {
      trouble = true;
    }
  }

  @Override
  public void write(char[] buf, int off, int len) {
    try {
      ensureOpen();
      out.write(buf, off, len);
    } catch (IOException e) {
      trouble = true;
    }
  }

  @Override
  public void write(char[] buf) {
    write(buf, 0, buf.length);
  }

  @Override
  public void write(String s, int off, int len) {
    try {
      ensureOpen();
      out.write(s, off, len);
    } catch (IOException e) {
      trouble = true;
    }
  }

  @Override
  public void write(String s) {
    try {
      ensureOpen();
      out.write(s);
    } catch (IOException e) {
      trouble = true;
    }
  }

  public void print(boolean b) {
    write(b ? "true" : "false");
  }

  public void print(char c) {
    write(c);
  }

  public void print(int i) {
    write("" + i);
  }

  public void print(long l) {
    write("" + l);
  }

  public void print(float f) {
    write("" + f);
  }

  public void print(double d) {
    write("" + d);
  }

  public void print(char[] s) {
    write(s);
  }

  public void print(String s) {
    write(s == null ? "null" : s);
  }

  public void print(Object obj) {
    write(obj == null ? "null" : obj.toString());
  }

  public void println() {
    write('\n');
    if (autoFlush) {
      flush();
    }
  }

  public void println(boolean x) {
    print(x);
    println();
  }

  public void println(char x) {
    print(x);
    println();
  }

  public void println(int x) {
    print(x);
    println();
  }

  public void println(long x) {
    print(x);
    println();
  }

  public void println(float x) {
    print(x);
    println();
  }

  public void println(double x) {
    print(x);
    println();
  }

  public void println(char[] x) {
    print(x);
    println();
  }

  public void println(String x) {
    print(x);
    println();
  }

  public void println(Object x) {
    print(x);
    println();
  }

  @Override
  public PrintWriter append(CharSequence csq) {
    write(csq == null ? "null" : csq.toString());
    return this;
  }

  @Override
  public PrintWriter append(char c) {
    write(c);
    return this;
  }
}
