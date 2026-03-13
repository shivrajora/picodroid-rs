package interfacedemo;

import picodroid.util.Log;

public abstract class Animal implements Speakable {
  public abstract int getSound();

  public void speak() {
    Log.i("Animal", "sound=" + getSound());
  }
}
