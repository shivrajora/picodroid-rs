// SPDX-License-Identifier: GPL-3.0-only
package picodroid.net;

/**
 * HTTP/1.1 client connection, Android-style.
 *
 * <p>Supports {@code GET}, {@code POST}, and {@code PUT}. Request bodies must have a known length,
 * set via {@link #setFixedLengthStreamingMode(int)}. HTTPS URLs are rejected at {@link #connect()}
 * time with {@link UnsupportedOperationException}.
 *
 * <pre>{@code
 * HttpURLConnection c = new URL("http://example.com/").openConnection();
 * c.connect();
 * if (c.getResponseCode() == 200) {
 *   HttpInputStream in = c.getInputStream();
 *   byte[] buf = new byte[256];
 *   int n;
 *   while ((n = in.read(buf)) > 0) { ... }
 * }
 * c.disconnect();
 * }</pre>
 */
public class HttpURLConnection implements AutoCloseable {
  private URL url;
  private String method;
  private boolean doOutput;
  private int fixedLength;
  private int handle;

  public HttpURLConnection(URL url) {
    this.url = url;
    this.method = "GET";
    this.doOutput = false;
    this.fixedLength = -1;
    this.handle = -1;
  }

  public void setRequestMethod(String m) {
    if (!m.equals("GET") && !m.equals("POST") && !m.equals("PUT")) {
      throw new UnsupportedOperationException("method not supported: " + m);
    }
    this.method = m;
  }

  public String getRequestMethod() {
    return method;
  }

  public void setDoOutput(boolean v) {
    this.doOutput = v;
  }

  /** Declares the exact byte length of the request body. Required for POST/PUT. */
  public void setFixedLengthStreamingMode(int len) {
    if (len < 0) {
      throw new IllegalArgumentException("length must be >= 0");
    }
    this.fixedLength = len;
  }

  public URL getURL() {
    return url;
  }

  /**
   * Resolve the host, open the TCP connection, and send the request line + headers. If {@code
   * doOutput} is true the connection is left ready for body writes via {@link #getOutputStream()};
   * otherwise the request is complete and {@link #getResponseCode()} can be called.
   */
  public void connect() {
    if (handle != -1) {
      return; // already connected
    }
    if (url.getProtocol().equals("https")) {
      throw new UnsupportedOperationException("HTTPS not yet supported");
    }
    if (doOutput && fixedLength < 0) {
      throw new IllegalStateException("setFixedLengthStreamingMode() required for output");
    }
    this.handle = nativeConnect(url.getHost(), url.getPort(), url.getPath(), method, fixedLength);
  }

  public HttpOutputStream getOutputStream() {
    if (handle == -1) {
      connect();
    }
    if (!doOutput) {
      throw new IllegalStateException("setDoOutput(true) required");
    }
    return new HttpOutputStream(handle);
  }

  public int getResponseCode() {
    if (handle == -1) {
      connect();
    }
    return nativeReadResponseCode(handle);
  }

  /** Returns the parsed {@code Content-Length}, or -1 if the server didn't send one. */
  public int getContentLength() {
    if (handle == -1) {
      return -1;
    }
    return nativeContentLength(handle);
  }

  public HttpInputStream getInputStream() {
    if (handle == -1) {
      connect();
    }
    // Force header parsing so the input stream starts at the body.
    nativeReadResponseCode(handle);
    return new HttpInputStream(handle);
  }

  public void disconnect() {
    if (handle != -1) {
      nativeDisconnect(handle);
      handle = -1;
    }
  }

  @Override
  public void close() {
    disconnect();
  }

  private static native int nativeConnect(
      String host, int port, String path, String method, int bodyLength);

  private static native int nativeReadResponseCode(int handle);

  private static native int nativeContentLength(int handle);

  private static native void nativeDisconnect(int handle);
}
