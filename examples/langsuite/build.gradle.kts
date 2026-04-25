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
            srcDir("../arraydemo/java")
            srcDir("../bytecodecoverage/java")
            srcDir("../enumdemo/java")
            srcDir("../exceptiondemo/java")
            srcDir("../floatdemo/java")
            srcDir("../hashmaptest/java")
            srcDir("../inherit/java")
            srcDir("../interfacedemo/java")
            srcDir("../iteratordemo/java")
            srcDir("../lambdademo/java")
            srcDir("../listdemo/java")
            srcDir("../mathsdemo/java")
            srcDir("../strformat/java")
            srcDir("../stringdemo/java")
            srcDir("../stringtest/java")
            srcDir("../syncdemo")
            srcDir("../trywithresourcesdemo")
        }
    }
}
