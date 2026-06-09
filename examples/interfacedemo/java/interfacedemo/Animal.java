// SPDX-License-Identifier: GPL-3.0-only
package interfacedemo;

import picodroid.util.Log;

public abstract class Animal implements Speakable {
  public abstract int getSound();

  @Override
  public void speak() {
    Log.i("Animal", "sound=" + getSound());
  }
}
