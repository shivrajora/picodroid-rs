plugins {
    id("picodroid-papk")
}

// The bridge's address is baked at build time:
// -PpicodroidNetTestHost=<ip> or PICODROID_NET_TEST_HOST, default loopback
// (which is what the simulator wants). See README.md.
picodroidNetTest {
    enabled = true
}
