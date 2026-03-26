package picodroid.pio;

public class Adc {
  private int pin;

  Adc(int pin) {
    this.pin = pin;
  }

  public native double readValue();

  public native void close();
}
