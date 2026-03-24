package interfacedemo;

public class InterfaceDemo {
  public static void main(String[] args) {
    // invokeinterface: dispatch speak() through the Speakable interface reference
    Speakable d = new Dog();
    d.speak(); // Dog extends Animal, Animal.speak() logs "sound=1"

    Speakable c = new Cat();
    c.speak(); // Cat extends Animal, Animal.speak() logs "sound=2"
  }
}
