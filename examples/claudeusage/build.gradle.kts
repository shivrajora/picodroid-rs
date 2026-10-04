plugins {
    id("picodroid-papk")
}

// The app finds the bridge by broadcast; this bakes the fallback address for
// a LAN that blocks broadcasts: -PpicodroidNetTestHost=<ip> or
// PICODROID_NET_TEST_HOST, default loopback. See README.md.
picodroidBuildConfig {
    fieldFromProperty("BRIDGE_HOST", "picodroidNetTestHost", "PICODROID_NET_TEST_HOST", "127.0.0.1")
}
