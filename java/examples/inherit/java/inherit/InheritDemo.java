package inherit;

import picodroid.util.Log;

public class InheritDemo {
    public static void main(String[] args) {
        Dog d = new Dog();

        // invokevirtual Dog.speak() — overridden in Dog: sound*10 + tricks = 1*10+3 = 13
        Log.i("INHERIT", "speak=" + d.speak());

        // invokevirtual Dog.getSound() — not in Dog, walks up to Animal
        Log.i("INHERIT", "sound=" + d.getSound());

        // getfield Dog.tricks — slot 1 (Animal.sound=slot0, Dog.tricks=slot1)
        Log.i("INHERIT", "tricks=" + d.tricks);
    }
}
