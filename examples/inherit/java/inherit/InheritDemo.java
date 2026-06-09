// SPDX-License-Identifier: GPL-3.0-only
package inherit;

import picodroid.app.Application;
import picodroid.util.Log;

public class InheritDemo extends Application {
  @Override
  public void onCreate() {
    run();
  }

  public static void run() {
    Dog d = new Dog();

    // invokevirtual Dog.speak() — overridden in Dog: sound*10 + tricks = 1*10+3 = 13
    Log.i("INHERIT", "speak=" + d.speak());

    // invokevirtual Dog.getSound() — not in Dog, walks up to Animal
    Log.i("INHERIT", "sound=" + d.getSound());

    // getfield Dog.tricks — slot 1 (Animal.sound=slot0, Dog.tricks=slot1)
    Log.i("INHERIT", "tricks=" + d.tricks);
  }
}
