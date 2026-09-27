plugins {
    id("picodroid-papk")
}

// Target host for the HTTPS demo is baked at build time (NET-7):
// -PpicodroidNetTestHost=<ip> or PICODROID_NET_TEST_HOST, default loopback.
picodroidNetTest {
    enabled = true
}
