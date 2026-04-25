package interfacedemo;

import picodroid.app.Application;

public class InterfaceDemo extends Application {
  public void onCreate() {
    run();
  }

  public static void run() {
    // invokeinterface: dispatch speak() through the Speakable interface reference
    Speakable d = new Dog();
    d.speak(); // Dog extends Animal, Animal.speak() logs "sound=1"

    Speakable c = new Cat();
    c.speak(); // Cat extends Animal, Animal.speak() logs "sound=2"
  }
}
