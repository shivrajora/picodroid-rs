plugins {
    id("picodroid-papk")
}

// Pull in the source dirs of every standalone demo this suite invokes so
// the merged PAPK contains all referenced classes. The picodroid-papk
// plugin sets srcDirs to projectDir; these calls append.
sourceSets {
    main {
        java {
            srcDir("../anondemo/java")
            srcDir("../bytecodecoverage/java")
            srcDir("../clonedemo/java")
            srcDir("../collectionsdemo/java")
            srcDir("../enumdemo/java")
            srcDir("../exceptiondemo/java")
            srcDir("../floatdemo/java")
            srcDir("../inherit/java")
            srcDir("../interfacedemo/java")
            srcDir("../lambdademo/java")
            srcDir("../mathsdemo/java")
            srcDir("../stringdemo/java")
            srcDir("../syncdemo")
            srcDir("../trywithresourcesdemo")
        }
    }
}
