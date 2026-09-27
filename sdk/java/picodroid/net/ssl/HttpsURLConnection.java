// SPDX-License-Identifier: GPL-3.0-only
package picodroid.net.ssl;

import picodroid.net.HttpURLConnection;
import picodroid.net.URL;

/**
 * An HTTPS connection, mirroring {@code javax.net.ssl.HttpsURLConnection}: an {@link
 * HttpURLConnection} whose bytes travel over TLS 1.3. {@link URL#openConnection()} returns one for
 * an {@code https} URL, so the usual cast works:
 *
 * <pre>{@code
 * HttpsURLConnection c = (HttpsURLConnection) new URL("https://api.example.com/v1").openConnection();
 * c.setRequestProperty("Accept", "application/json");
 * int status = c.getResponseCode();
 * }</pre>
 *
 * <p>What {@link #connect()} does on the wire: the server name is sent (SNI) and checked against
 * the certificate; the chain is verified against the runtime's compiled-in root store; validity is
 * checked against the wall clock, which must have been set — sync it first, for example with {@code
 * picodroid.net.SntpClient}. A failed check throws {@link javax.net.ssl.SSLHandshakeException}; a
 * handshake that ran into the read timeout throws {@link java.net.SocketTimeoutException}. All are
 * {@link java.io.IOException}s, as on Android.
 *
 * <p>Not mirrored: {@code setSSLSocketFactory}, {@code setHostnameVerifier}, {@code
 * getServerCertificates}. The runtime negotiates one cipher suite with one trust store, and there
 * is no way to loosen either from an app.
 *
 * <p>Boards built without TLS ({@code has_tls} unset) throw {@link UnsupportedOperationException}
 * from {@link #connect()}.
 */
public class HttpsURLConnection extends HttpURLConnection {
  public HttpsURLConnection(URL url) {
    super(url);
  }

  /** The cipher suite every picodroid TLS session negotiates. */
  public String getCipherSuite() {
    return "TLS_AES_128_GCM_SHA256";
  }
}
