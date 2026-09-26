// SPDX-License-Identifier: GPL-3.0-only
package picodroid.net;

/**
 * What a callback wants of a network, mirroring {@code android.net.NetworkRequest}: built with
 * {@link Builder} and handed to {@link ConnectivityManager#registerNetworkCallback}. A network
 * satisfies a request when it has every capability the request asks for and, if the request names
 * transport types, one of them.
 *
 * <pre>{@code
 * NetworkRequest wifi = new NetworkRequest.Builder()
 *     .addTransportType(NetworkCapabilities.TRANSPORT_WIFI)
 *     .addCapability(NetworkCapabilities.NET_CAPABILITY_INTERNET)
 *     .build();
 * }</pre>
 */
public final class NetworkRequest {
  /** Android's default request asks for these; {@link Builder} starts from them. */
  private static final int DEFAULT_CAPABILITIES =
      (1 << NetworkCapabilities.NET_CAPABILITY_INTERNET)
          | (1 << NetworkCapabilities.NET_CAPABILITY_NOT_RESTRICTED)
          | (1 << NetworkCapabilities.NET_CAPABILITY_TRUSTED)
          | (1 << NetworkCapabilities.NET_CAPABILITY_NOT_VPN);

  final int transports;
  final int capabilities;

  NetworkRequest(int transports, int capabilities) {
    this.transports = transports;
    this.capabilities = capabilities;
  }

  /** Whether the request names {@code transportType}. */
  public boolean hasTransport(int transportType) {
    return transportType >= 0 && transportType < 32 && (transports & (1 << transportType)) != 0;
  }

  /** Whether the request asks for {@code capability}. */
  public boolean hasCapability(int capability) {
    return capability >= 0 && capability < 32 && (capabilities & (1 << capability)) != 0;
  }

  /** Whether a network with {@code nc} satisfies this request. */
  boolean matches(NetworkCapabilities nc) {
    if (nc == null || (nc.capabilities & capabilities) != capabilities) {
      return false;
    }
    return transports == 0 || (nc.transports & transports) != 0;
  }

  /** Builds a {@link NetworkRequest}, mirroring {@code android.net.NetworkRequest.Builder}. */
  public static final class Builder {
    private int transports;
    private int capabilities = DEFAULT_CAPABILITIES;

    public Builder() {}

    /**
     * Also accept networks on {@code transportType} (a {@code NetworkCapabilities.TRANSPORT_*}
     * value). With none named, any transport does.
     */
    public Builder addTransportType(int transportType) {
      transports |= bit(transportType, "transport type");
      return this;
    }

    public Builder removeTransportType(int transportType) {
      transports &= ~bit(transportType, "transport type");
      return this;
    }

    /** Require {@code capability} (a {@code NetworkCapabilities.NET_CAPABILITY_*} value). */
    public Builder addCapability(int capability) {
      capabilities |= bit(capability, "capability");
      return this;
    }

    public Builder removeCapability(int capability) {
      capabilities &= ~bit(capability, "capability");
      return this;
    }

    /** Drop the default capabilities too, so the request asks for nothing but what is added. */
    public Builder clearCapabilities() {
      capabilities = 0;
      return this;
    }

    public NetworkRequest build() {
      return new NetworkRequest(transports, capabilities);
    }

    private static int bit(int value, String what) {
      if (value < 0 || value >= 32) {
        throw new IllegalArgumentException(what + " out of range: " + value);
      }
      return 1 << value;
    }
  }
}
