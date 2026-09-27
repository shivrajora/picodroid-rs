plugins {
    id("picodroid-papk")
}

// The nightly's canned forecast lives on the test host's TLS listener
// (API_URL=test in test.env); NetTestConfig.HOST names that host.
picodroidNetTest {
    enabled = true
}

// The place and the units are build-time constants (Android's buildConfigField
// shape), each from a Gradle property, the environment, or the default:
//   PICODROID_WEATHER_CITY=Tokyo PICODROID_WEATHER_LATITUDE=35.68 \
//   PICODROID_WEATHER_LONGITUDE=139.69 ./scripts/flash.sh --app weather --board pico_enviro_mon_w
// PICODROID_WEATHER_UNITS=fahrenheit switches to °F and mph.
picodroidBuildConfig {
    fieldFromProperty("CITY", "picodroidWeatherCity", "PICODROID_WEATHER_CITY", "San Mateo")
    fieldFromProperty("LATITUDE", "picodroidWeatherLatitude", "PICODROID_WEATHER_LATITUDE", "37.56")
    fieldFromProperty("LONGITUDE", "picodroidWeatherLongitude", "PICODROID_WEATHER_LONGITUDE", "-122.32")
    fieldFromProperty("UNITS", "picodroidWeatherUnits", "PICODROID_WEATHER_UNITS", "celsius")
    fieldFromProperty("API_URL", "picodroidWeatherUrl", "PICODROID_WEATHER_URL", "https://api.open-meteo.com/v1/forecast")
}
