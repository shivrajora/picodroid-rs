// SPDX-License-Identifier: GPL-3.0-only
package qa_io;

import java.io.BufferedReader;
import java.io.ByteArrayInputStream;
import java.io.ByteArrayOutputStream;
import java.io.Closeable;
import java.io.IOException;
import java.io.InputStream;
import java.io.InputStreamReader;
import java.io.OutputStream;
import java.io.OutputStreamWriter;
import java.io.PrintWriter;
import java.io.Reader;
import java.io.Writer;
import picodroid.app.Application;
import picodroid.content.Context;
import picodroid.io.FileInputStream;
import picodroid.io.FileOutputStream;
import picodroid.util.Log;

/**
 * QA 2026-10-09: the {@code java.io} stream hierarchy (android-parity roadmap T3.3) — the byte
 * array streams, the base-class defaults an app's own stream inherits, readers and line splitting,
 * writers, and the file streams seen through their {@code java.io} supertypes. Every check names
 * what it pins; a section that throws is reported and the run continues.
 */
public class QaIo extends Application {
  private static final String TAG = "QaIo";

  static int passed = 0;
  static int failed = 0;
  static int crashed = 0;

  static void check(String name, boolean condition) {
    if (condition) {
      passed = passed + 1;
    } else {
      Log.i(TAG, "FAIL: " + name);
      failed = failed + 1;
    }
  }

  interface Section {
    void run() throws IOException;
  }

  static void section(String name, Section s) {
    Log.i(TAG, "section " + name);
    try {
      s.run();
    } catch (Throwable t) {
      Log.i(TAG, "CRASH in " + name + ": " + t + " msg=" + t.getMessage());
      crashed = crashed + 1;
    }
  }

  @Override
  public void onCreate() {
    Log.i(TAG, "=== QaIo start ===");
    section("byteArrays", () -> byteArrays());
    section("baseDefaults", () -> baseDefaults());
    section("readers", () -> readers());
    section("writers", () -> writers());
    section("files", () -> files());
    section("hierarchy", () -> hierarchy());
    Log.i(TAG, "passed=" + passed + " failed=" + failed + " crashed=" + crashed);
    if (failed == 0 && crashed == 0) {
      Log.i(TAG, "=== ALL PASSED ===");
    } else {
      Log.i(TAG, "=== FAILED: " + failed + " failed, " + crashed + " crashed ===");
    }
  }

  static byte[] ascii(String s) {
    return s.getBytes();
  }

  /** Drains {@code in} through the base-class {@code read(byte[])} into a byte array stream. */
  static byte[] drain(InputStream in) throws IOException {
    ByteArrayOutputStream out = new ByteArrayOutputStream();
    byte[] buf = new byte[7];
    int n;
    while ((n = in.read(buf)) != -1) {
      out.write(buf, 0, n);
    }
    return out.toByteArray();
  }

  // ---- ByteArrayInputStream / ByteArrayOutputStream -----------------------------------------

  static void byteArrays() throws IOException {
    ByteArrayOutputStream out = new ByteArrayOutputStream(4);
    out.write('a');
    out.write(ascii("bcdefgh"));
    out.write(ascii("xxijkxx"), 2, 3);
    check("baos size", out.size() == 11);
    check("baos toString", out.toString().equals("abcdefghijk"));
    byte[] bytes = out.toByteArray();
    check("baos toByteArray", bytes.length == 11 && bytes[0] == 'a' && bytes[10] == 'k');
    out.write(0xC3);
    out.write(0xA9);
    byte[] withHigh = out.toByteArray();
    check(
        "baos keeps bytes above 0x7F",
        (withHigh[11] & 0xff) == 0xC3 && (withHigh[12] & 0xff) == 0xA9);
    out.reset();
    check("baos reset", out.size() == 0 && out.toString().equals(""));

    ByteArrayInputStream in = new ByteArrayInputStream(bytes);
    check("bais available", in.available() == 11);
    check("bais read()", in.read() == 'a');
    byte[] four = new byte[4];
    check("bais read(byte[])", in.read(four) == 4 && four[0] == 'b' && four[3] == 'e');
    check("bais skip", in.skip(2) == 2 && in.read() == 'h');
    check("bais markSupported", in.markSupported());
    in.mark(0);
    check("bais read after mark", in.read() == 'i');
    in.reset();
    check("bais reset", in.read() == 'i');
    check("bais short read at end", in.read(four, 0, 4) == 2 && four[0] == 'j' && four[1] == 'k');
    check("bais eof", in.read() == -1 && in.read(four) == -1 && in.available() == 0);
    // The JDK's ByteArrayInputStream answers -1 at end of stream even for a zero-length read.
    check("bais zero-length read at eof", in.read(four, 0, 0) == -1);
    ByteArrayInputStream high = new ByteArrayInputStream(withHigh, 11, 2);
    check(
        "bais read() is unsigned", high.read() == 0xC3 && high.read() == 0xA9 && high.read() == -1);
  }

  // ---- what an app's own stream inherits from InputStream / OutputStream --------------------

  /** A stream that implements only {@code read()}: every other read is the base class's. */
  static final class CountingStream extends InputStream {
    private int next;
    private final int limit;

    CountingStream(int limit) {
      this.limit = limit;
    }

    @Override
    public int read() {
      return next < limit ? (next++ & 0xff) : -1;
    }
  }

  /** A sink that implements only {@code write(int)}. */
  static final class SummingStream extends OutputStream {
    int sum;
    int count;

    @Override
    public void write(int b) {
      sum += b & 0xff;
      count++;
    }
  }

  static void baseDefaults() throws IOException {
    CountingStream in = new CountingStream(10);
    byte[] buf = new byte[4];
    check("InputStream.read(byte[]) via read()", in.read(buf) == 4 && buf[0] == 0 && buf[3] == 3);
    check("InputStream.skip via read()", in.skip(3) == 3);
    check(
        "InputStream.read(byte[],off,len)", in.read(buf, 1, 2) == 2 && buf[1] == 7 && buf[2] == 8);
    check("InputStream short read", in.read(buf) == 1 && buf[0] == 9);
    check("InputStream eof", in.read(buf) == -1);
    check("InputStream.available default", in.available() == 0);
    check("InputStream.markSupported default", !in.markSupported());
    boolean threw = false;
    try {
      in.reset();
    } catch (IOException e) {
      threw = true;
    }
    check("InputStream.reset default throws", threw);
    byte[] all = drain(new CountingStream(20));
    check("drain through the base read", all.length == 20 && all[19] == 19);

    SummingStream sink = new SummingStream();
    sink.write(ascii("abc"));
    sink.write(ascii("xdefx"), 1, 3);
    sink.write(1);
    check(
        "OutputStream.write(byte[]) via write(int)",
        sink.count == 7 && sink.sum == 'a' + 'b' + 'c' + 'd' + 'e' + 'f' + 1);
    sink.flush();
    sink.close();
    boolean bounds = false;
    try {
      sink.write(buf, 2, 4);
    } catch (IndexOutOfBoundsException e) {
      bounds = true;
    }
    check("OutputStream.write bounds", bounds);
  }

  // ---- InputStreamReader / BufferedReader ---------------------------------------------------

  static BufferedReader readerOver(String text) {
    return new BufferedReader(new InputStreamReader(new ByteArrayInputStream(ascii(text))));
  }

  static void readers() throws IOException {
    BufferedReader r = readerOver("one\ntwo\r\nthree\rfour\n\nsix");
    check("readLine \\n", "one".equals(r.readLine()));
    check("readLine \\r\\n", "two".equals(r.readLine()));
    check("readLine \\r", "three".equals(r.readLine()));
    check("readLine after \\r", "four".equals(r.readLine()));
    check("readLine empty line", "".equals(r.readLine()));
    check("readLine final unterminated", "six".equals(r.readLine()));
    String atEof = r.readLine();
    String stillAtEof = r.readLine();
    check("readLine null at eof", atEof == null && stillAtEof == null);
    r.close();
    boolean closed = false;
    try {
      r.readLine();
    } catch (IOException e) {
      closed = true;
    }
    check("readLine after close throws", closed);

    check("readLine on empty input", readerOver("").readLine() == null);
    check("readLine lone newline", "".equals(readerOver("\n").readLine()));
    BufferedReader crlf = readerOver("a\r\n");
    check("readLine trailing crlf", "a".equals(crlf.readLine()) && crlf.readLine() == null);

    StringBuilder longLine = new StringBuilder();
    for (int i = 0; i < 300; i++) {
      longLine.append((char) ('a' + (i % 26)));
    }
    String longText = longLine.toString();
    BufferedReader big = readerOver(longText + "\nend");
    String got = big.readLine();
    check(
        "readLine longer than the buffer",
        got != null && got.length() == 300 && got.equals(longText));
    check("readLine after a long line", "end".equals(big.readLine()));

    InputStreamReader isr = new InputStreamReader(new ByteArrayInputStream(ascii("hello world")));
    check("InputStreamReader.read()", isr.read() == 'h');
    char[] cb = new char[4];
    check("Reader.read(char[])", isr.read(cb) == 4 && cb[0] == 'e' && cb[3] == 'o');
    check("Reader.skip", isr.skip(1) == 1 && isr.read() == 'w');
    check("InputStreamReader.ready", isr.ready());
    check("InputStreamReader.getEncoding", "UTF-8".equals(isr.getEncoding()));
    InputStreamReader named =
        new InputStreamReader(new ByteArrayInputStream(ascii("x")), "ISO-8859-1");
    check("charset name accepted", "ISO-8859-1".equals(named.getEncoding()) && named.read() == 'x');

    byte[] utf8 = new byte[] {'c', 'a', 'f', (byte) 0xC3, (byte) 0xA9, '\n'};
    BufferedReader accents =
        new BufferedReader(new InputStreamReader(new ByteArrayInputStream(utf8)), 2);
    String cafe = accents.readLine();
    byte[] back = cafe.getBytes();
    check(
        "readLine keeps UTF-8 bytes",
        back.length == 5 && (back[3] & 0xff) == 0xC3 && (back[4] & 0xff) == 0xA9);

    BufferedReader chunks = readerOver("abcdef");
    char[] six = new char[6];
    check("BufferedReader.read(char[],off,len)", chunks.read(six, 0, 3) == 3 && six[2] == 'c');
    check("BufferedReader.read()", chunks.read() == 'd');
    check("BufferedReader.ready", chunks.ready());
    check("BufferedReader rest", chunks.read(six, 0, 6) == 2 && six[0] == 'e' && six[1] == 'f');
    check("BufferedReader eof", chunks.read(six) == -1 && chunks.read() == -1);
  }

  // ---- OutputStreamWriter / PrintWriter -----------------------------------------------------

  static void writers() throws IOException {
    ByteArrayOutputStream bytes = new ByteArrayOutputStream();
    Writer w = new OutputStreamWriter(bytes);
    w.write("ab");
    w.write('c');
    w.write(new char[] {'d', 'e', 'f'}, 1, 2);
    w.append("gh").append('i');
    w.write("xjkx", 1, 2);
    w.flush();
    check("OutputStreamWriter bytes", bytes.toString().equals("abcefghijk"));
    w.close();

    ByteArrayOutputStream sink = new ByteArrayOutputStream();
    PrintWriter pw = new PrintWriter(sink, true);
    pw.print("n=");
    pw.println(42);
    pw.println("line");
    pw.print(true);
    pw.print(' ');
    pw.print(7L);
    pw.print(' ');
    pw.print((Object) null);
    pw.println();
    pw.println(new char[] {'o', 'k'});
    pw.write("end");
    pw.flush();
    check("PrintWriter text", sink.toString().equals("n=42\nline\ntrue 7 null\nok\nend"));
    check("PrintWriter.checkError clean", !pw.checkError());
    pw.close();
    pw.println("after close");
    check("PrintWriter after close sets the error flag", pw.checkError());

    ByteArrayOutputStream fp = new ByteArrayOutputStream();
    PrintWriter nums = new PrintWriter(new OutputStreamWriter(fp));
    nums.print(1.5);
    nums.flush();
    check("PrintWriter.print(double)", fp.toString().equals("1.5"));
  }

  // ---- the file streams as java.io streams --------------------------------------------------

  void files() throws IOException {
    FileOutputStream raw = openFileOutput("qa_io.txt", Context.MODE_PRIVATE);
    check("FileOutputStream is an OutputStream", raw instanceof OutputStream);
    PrintWriter out = new PrintWriter(raw);
    out.println("first line");
    out.println("second");
    out.print("third");
    out.close();
    check("PrintWriter over a file: no error", !out.checkError());

    FileInputStream fis = openFileInput("qa_io.txt");
    check("FileInputStream is an InputStream", fis instanceof InputStream);
    BufferedReader lines = new BufferedReader(new InputStreamReader(fis));
    check("file readLine 1", "first line".equals(lines.readLine()));
    check("file readLine 2", "second".equals(lines.readLine()));
    check("file readLine 3", "third".equals(lines.readLine()));
    check("file readLine eof", lines.readLine() == null);
    lines.close();

    InputStream in = openFileInput("qa_io.txt");
    check("FileInputStream.read() byte", in.read() == 'f');
    check("InputStream.skip on a file", in.skip(5) == 5 && in.read() == 'l');
    byte[] rest = drain(in);
    check(
        "drain a file through InputStream", rest.length == 16 && rest[0] == 'i' && rest[15] == 'd');
    in.close();

    try (InputStream twr = openFileInput("qa_io.txt")) {
      check("try-with-resources on InputStream", twr.read() == 'f');
    }
    Closeable c = openFileInput("qa_io.txt");
    c.close();
    check("FileInputStream as Closeable", true);

    OutputStream append = openFileOutput("qa_io.txt", Context.MODE_APPEND);
    append.write('!');
    append.flush();
    append.close();
    Reader r = new InputStreamReader(openFileInput("qa_io.txt"));
    BufferedReader last = new BufferedReader(r);
    last.readLine();
    last.readLine();
    check("appended through OutputStream.write(int)", "third!".equals(last.readLine()));
    last.close();
  }

  // ---- instanceof / supertypes ---------------------------------------------------------------

  static void hierarchy() throws IOException {
    InputStream in = new ByteArrayInputStream(ascii("x"));
    check("InputStream is Closeable", in instanceof Closeable);
    check("InputStream is AutoCloseable", in instanceof AutoCloseable);
    Reader r = new InputStreamReader(in);
    check("Reader is Closeable", r instanceof Closeable);
    check("BufferedReader is a Reader", new BufferedReader(r) instanceof Reader);
    Writer w = new PrintWriter(new ByteArrayOutputStream());
    check("PrintWriter is a Writer and AutoCloseable", w instanceof AutoCloseable);
    Object o = new ByteArrayOutputStream();
    check("ByteArrayOutputStream is an OutputStream", o instanceof OutputStream);
    check("a Reader is not an InputStream", !(((Object) r) instanceof InputStream));
  }
}
