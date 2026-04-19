package picodroid.net;

/**
 * A parsed HTTP URL.
 *
 * <p>Accepts {@code http://host[:port][/path]} and {@code https://host[:port][/path]}. Hostnames
 * and dotted-quad IPv4 literals are both allowed; DNS resolution happens later, at connect time.
 * Query strings belong in the path — there is no separate query accessor.
 */
public class Url {
  private String protocol;
  private String host;
  private int port;
  private String path;

  public Url(String spec) {
    if (spec == null) {
      throw new IllegalArgumentException("url is null");
    }
    int schemeEnd = spec.indexOf("://");
    if (schemeEnd <= 0) {
      throw new IllegalArgumentException("missing scheme");
    }
    this.protocol = spec.substring(0, schemeEnd);
    if (!this.protocol.equals("http") && !this.protocol.equals("https")) {
      throw new IllegalArgumentException("unsupported scheme: " + this.protocol);
    }

    int hostStart = schemeEnd + 3;
    int pathStart = -1;
    for (int i = hostStart; i < spec.length(); i++) {
      if (spec.charAt(i) == '/') {
        pathStart = i;
        break;
      }
    }
    String authority;
    if (pathStart < 0) {
      authority = spec.substring(hostStart, spec.length());
      this.path = "/";
    } else {
      authority = spec.substring(hostStart, pathStart);
      this.path = spec.substring(pathStart, spec.length());
    }
    if (authority.length() == 0) {
      throw new IllegalArgumentException("missing host");
    }

    int colon = authority.indexOf(':');
    if (colon < 0) {
      this.host = authority;
      this.port = this.protocol.equals("https") ? 443 : 80;
    } else {
      this.host = authority.substring(0, colon);
      String portStr = authority.substring(colon + 1, authority.length());
      this.port = parseUnsignedInt(portStr);
      if (this.port < 1 || this.port > 65535) {
        throw new IllegalArgumentException("port out of range: " + portStr);
      }
    }
    if (this.host.length() == 0) {
      throw new IllegalArgumentException("missing host");
    }
  }

  public String getProtocol() {
    return protocol;
  }

  public String getHost() {
    return host;
  }

  public int getPort() {
    return port;
  }

  public String getPath() {
    return path;
  }

  /** Open an HTTP connection to this URL. The connection is not opened until {@code connect()}. */
  public HttpUrlConnection openConnection() {
    return new HttpUrlConnection(this);
  }

  // Local decimal parser — Integer.parseInt isn't wired up in the picodroid JVM.
  private static int parseUnsignedInt(String s) {
    int n = s.length();
    if (n == 0) {
      throw new IllegalArgumentException("empty number");
    }
    int v = 0;
    for (int i = 0; i < n; i++) {
      char c = s.charAt(i);
      if (c < '0' || c > '9') {
        throw new IllegalArgumentException("non-digit: " + s);
      }
      v = v * 10 + (c - '0');
      if (v > 0xFFFFFF) {
        throw new IllegalArgumentException("overflow: " + s);
      }
    }
    return v;
  }
}
