plugins {
    `kotlin-dsl`
}

repositories {
    mavenCentral()
}

gradlePlugin {
    plugins {
        create("picodroid-papk") {
            id = "picodroid-papk"
            implementationClass = "picodroid.PicodroidPapkPlugin"
        }
    }
}
