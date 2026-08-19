// SPDX-License-Identifier: GPL-3.0-only
package picodroid.net;

import java.io.IOException;

/**
 * HTTP/1.1 client connection, Android-style.
 *
 * <p>Supports {@code GET}, {@code POST}, and {@code PUT}. Request bodies must have a known length,
 * set via {@link #setFixedLengthStreamingMode(int)}. HTTPS URLs are rejected at {@link #connect()}
 * time with {@link UnsupportedOperationException}.
 *
 * <p>Request headers are set with {@link #setRequestProperty(String, String)} before connecting;
 * response headers are read with {@link #getHeaderField(String)}. {@code Host}, {@code Connection},
 * and {@code Content-Length} are managed by the connection itself.
 *
 * <pre>{@code
 * HttpURLConnection c = new URL("http://example.com/").openConnection();
 * c.setRequestProperty("Accept", "application/json");
 * c.connect();
 * if (c.getResponseCode() == HttpURLConnection.HTTP_OK) {
 *   String type = c.getHeaderField("Content-Type");
 *   HttpInputStream in = c.getInputStream();
 *   byte[] buf = new byte[256];
 *   int n;
 *   while ((n = in.read(buf)) > 0) { ... }
 * }
 * c.disconnect();
 * }</pre>
 */
public class HttpURLConnection implements AutoCloseable {
  /** HTTP status 200: the request succeeded. */
  public static final int HTTP_OK = 200;

  /** HTTP status 201: the request succeeded and created a resource. */
  public static final int HTTP_CREATED = 201;

  /** HTTP status 202: the request was accepted for processing. */
  public static final int HTTP_ACCEPTED = 202;

  /** HTTP status 204: the request succeeded with no body to return. */
  public static final int HTTP_NO_CONTENT = 204;

  /** HTTP status 301: the resource moved permanently. */
  public static final int HTTP_MOVED_PERM = 301;

  /** HTTP status 302: the resource moved temporarily. */
  public static final int HTTP_MOVED_TEMP = 302;

  /** HTTP status 304: the cached copy is still current. */
  public static final int HTTP_NOT_MODIFIED = 304;

  /** HTTP status 400: the server could not understand the request. */
  public static final int HTTP_BAD_REQUEST = 400;

  /** HTTP status 401: the request requires authentication. */
  public static final int HTTP_UNAUTHORIZED = 401;

  /** HTTP status 403: the server refused to fulfil the request. */
  public static final int HTTP_FORBIDDEN = 403;

  /** HTTP status 404: no resource matches the request URI. */
  public static final int HTTP_NOT_FOUND = 404;

  /** HTTP status 500: the server hit an unexpected error. */
  public static final int HTTP_INTERNAL_ERROR = 500;

  /** HTTP status 503: the server is temporarily unavailable. */
  public static final int HTTP_UNAVAILABLE = 503;

  /**
   * Maximum number of request headers. Requests are assembled into a fixed-size buffer on the
   * native side, so the cap keeps a runaway loop from overflowing it.
   */
  private static final int MAX_REQUEST_HEADERS = 16;

  private URL url;
  private String method;
  private boolean doOutput;
  private int fixedLength;
  private int connectTimeout;
  private int readTimeout;
  private int handle;
  private String[] headerNames;
  private String[] headerValues;
  private int headerCount;

  public HttpURLConnection(URL url) {
    this.url = url;
    this.method = "GET";
    this.doOutput = false;
    this.fixedLength = -1;
    this.connectTimeout = 0;
    this.readTimeout = 0;
    this.handle = -1;
    this.headerNames = new String[MAX_REQUEST_HEADERS];
    this.headerValues = new String[MAX_REQUEST_HEADERS];
    this.headerCount = 0;
  }

  /**
   * Sets the connect timeout in milliseconds. A timeout of zero (the default) is interpreted as an
   * infinite timeout. On expiry, {@link #connect()} throws {@link java.net.SocketTimeoutException}.
   *
   * @throws IllegalArgumentException if {@code timeoutMs} is negative
   */
  public void setConnectTimeout(int timeoutMs) {
    if (timeoutMs < 0) {
      throw new IllegalArgumentException("timeout can not be negative");
    }
    this.connectTimeout = timeoutMs;
  }

  public int getConnectTimeout() {
    return connectTimeout;
  }

  /**
   * Sets the read timeout in milliseconds, bounding each blocking read of the response — {@link
   * #getResponseCode()} and every {@code HttpInputStream.read}. A timeout of zero (the default) is
   * interpreted as an infinite timeout. On expiry the read throws {@link
   * java.net.SocketTimeoutException}.
   *
   * <p>Must be set before {@link #connect()}; the value is applied to the underlying socket at
   * connect time.
   *
   * @throws IllegalArgumentException if {@code timeoutMs} is negative
   */
  public void setReadTimeout(int timeoutMs) {
    if (timeoutMs < 0) {
      throw new IllegalArgumentException("timeout can not be negative");
    }
    this.readTimeout = timeoutMs;
  }

  public int getReadTimeout() {
    return readTimeout;
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
   * Sets a request header, replacing any existing value for {@code name}.
   *
   * <p>Must be called before {@link #connect()} — headers are written when the request is sent.
   * {@code Host}, {@code Connection}, and {@code Content-Length} are managed by the connection;
   * values set for them here are ignored.
   *
   * @throws IllegalArgumentException if {@code name} is null or empty, or if either argument
   *     contains a carriage return or newline
   * @throws IllegalStateException if already connected, or if more than 16 headers are set
   */
  public void setRequestProperty(String name, String value) {
    checkHeader(name, value);
    for (int i = 0; i < headerCount; i++) {
      if (headerNames[i].equalsIgnoreCase(name)) {
        headerValues[i] = value;
        return;
      }
    }
    appendHeader(name, value);
  }

  /**
   * Adds a request header, keeping any existing value for {@code name} — the request carries one
   * line per call. Same restrictions as {@link #setRequestProperty(String, String)}.
   */
  public void addRequestProperty(String name, String value) {
    checkHeader(name, value);
    appendHeader(name, value);
  }

  /**
   * Returns the value set for the request header {@code name} (case-insensitive), or null if unset.
   * When a header was added more than once, the first value is returned.
   */
  public String getRequestProperty(String name) {
    if (name == null) {
      return null;
    }
    for (int i = 0; i < headerCount; i++) {
      if (headerNames[i].equalsIgnoreCase(name)) {
        return headerValues[i];
      }
    }
    return null;
  }

  private void checkHeader(String name, String value) {
    if (handle != -1) {
      throw new IllegalStateException("cannot set request property after connection is made");
    }
    if (name == null || name.isEmpty()) {
      throw new IllegalArgumentException("header name must not be empty");
    }
    // Reject CR/LF outright: they would otherwise let a caller inject extra
    // header lines (or a whole body) into the request we assemble.
    if (containsCrLf(name) || (value != null && containsCrLf(value))) {
      throw new IllegalArgumentException("header must not contain CR or LF");
    }
  }

  private static boolean containsCrLf(String s) {
    return s.indexOf('\r') >= 0 || s.indexOf('\n') >= 0;
  }

  private void appendHeader(String name, String value) {
    if (headerCount == MAX_REQUEST_HEADERS) {
      throw new IllegalStateException("too many request headers (max " + MAX_REQUEST_HEADERS + ")");
    }
    headerNames[headerCount] = name;
    headerValues[headerCount] = value;
    headerCount = headerCount + 1;
  }

  /**
   * Render the caller's headers as {@code K: V\r\n} lines for the native request builder, skipping
   * the three the connection sets itself.
   */
  private String buildRequestHeaders() {
    StringBuilder sb = new StringBuilder();
    for (int i = 0; i < headerCount; i++) {
      String name = headerNames[i];
      if (name.equalsIgnoreCase("Host")
          || name.equalsIgnoreCase("Connection")
          || name.equalsIgnoreCase("Content-Length")) {
        continue;
      }
      sb.append(name);
      sb.append(": ");
      sb.append(headerValues[i] == null ? "" : headerValues[i]);
      sb.append("\r\n");
    }
    return sb.toString();
  }

  /**
   * Resolve the host, open the TCP connection, and send the request line + headers. If {@code
   * doOutput} is true the connection is left ready for body writes via {@link #getOutputStream()};
   * otherwise the request is complete and {@link #getResponseCode()} can be called.
   *
   * @throws java.net.UnknownHostException if the host cannot be resolved
   * @throws java.net.ConnectException if the server actively refused the connection
   * @throws java.net.SocketTimeoutException if the connect attempt timed out
   * @throws IOException for any other connection or request-send failure
   */
  public void connect() throws IOException {
    if (handle != -1) {
      return; // already connected
    }
    if (url.getProtocol().equals("https")) {
      throw new UnsupportedOperationException("HTTPS not yet supported");
    }
    if (doOutput && fixedLength < 0) {
      throw new IllegalStateException("setFixedLengthStreamingMode() required for output");
    }
    this.handle =
        nativeConnect(
            url.getHost(),
            url.getPort(),
            url.getPath(),
            method,
            fixedLength,
            connectTimeout,
            readTimeout,
            buildRequestHeaders());
  }

  public HttpOutputStream getOutputStream() throws IOException {
    if (handle == -1) {
      connect();
    }
    if (!doOutput) {
      throw new IllegalStateException("setDoOutput(true) required");
    }
    return new HttpOutputStream(handle);
  }

  /**
   * @throws java.net.ProtocolException if the server's response is not parseable HTTP
   * @throws IOException if connecting or reading the response head fails
   */
  public int getResponseCode() throws IOException {
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

  public HttpInputStream getInputStream() throws IOException {
    if (handle == -1) {
      connect();
    }
    // Force header parsing so the input stream starts at the body.
    nativeReadResponseCode(handle);
    return new HttpInputStream(handle);
  }

  /**
   * Returns the body stream for an error response — status 400 and above — or null if the response
   * succeeded. Lets a caller read a server's error payload after {@link #getInputStream()} would
   * have been the wrong thing to reach for.
   *
   * @throws IOException if connecting or reading the response head fails
   */
  public HttpInputStream getErrorStream() throws IOException {
    if (handle == -1) {
      return null;
    }
    if (getResponseCode() < HTTP_BAD_REQUEST) {
      return null;
    }
    return new HttpInputStream(handle);
  }

  /**
   * Returns the reason phrase from the status line ({@code HTTP/1.1 404 Not Found} yields {@code
   * Not Found}), or null if the response head has not been read.
   *
   * @throws IOException if connecting or reading the response head fails
   */
  public String getResponseMessage() throws IOException {
    if (handle == -1) {
      connect();
    }
    nativeReadResponseCode(handle);
    return nativeResponseMessage(handle);
  }

  /**
   * Returns the value of the response header {@code name} (case-insensitive), or null if the server
   * did not send it. When the header appears more than once the last value wins.
   *
   * @throws IOException if connecting or reading the response head fails
   */
  public String getHeaderField(String name) throws IOException {
    if (handle == -1) {
      connect();
    }
    nativeReadResponseCode(handle);
    return nativeHeaderField(handle, name);
  }

  /**
   * Returns the value of the {@code n}th response header, or null once {@code n} is past the last
   * one. Index 0 is the status line, whose key is null — matching {@code
   * java.net.HttpURLConnection}, so iterating from 1 walks the real headers.
   *
   * @throws IOException if connecting or reading the response head fails
   */
  public String getHeaderField(int n) throws IOException {
    if (handle == -1) {
      connect();
    }
    nativeReadResponseCode(handle);
    return nativeHeaderFieldAt(handle, n, false);
  }

  /**
   * Returns the name of the {@code n}th response header, null for index 0 (the status line) and
   * null once {@code n} is past the last header.
   *
   * @throws IOException if connecting or reading the response head fails
   */
  public String getHeaderFieldKey(int n) throws IOException {
    if (handle == -1) {
      connect();
    }
    nativeReadResponseCode(handle);
    return nativeHeaderFieldAt(handle, n, true);
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
      String host,
      int port,
      String path,
      String method,
      int bodyLength,
      int connectTimeoutMs,
      int readTimeoutMs,
      String extraHeaders)
      throws IOException;

  private static native int nativeReadResponseCode(int handle) throws IOException;

  private static native int nativeContentLength(int handle);

  private static native String nativeHeaderField(int handle, String name);

  private static native String nativeHeaderFieldAt(int handle, int n, boolean wantKey);

  private static native String nativeResponseMessage(int handle);

  private static native void nativeDisconnect(int handle);
}
