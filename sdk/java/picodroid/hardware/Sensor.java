package picodroid.hardware;

public final class Sensor {
  public static final int TYPE_ALL = -1;
  public static final int TYPE_LIGHT = 5;
  public static final int TYPE_PRESSURE = 6;
  public static final int TYPE_PROXIMITY = 8;
  public static final int TYPE_RELATIVE_HUMIDITY = 12;
  public static final int TYPE_AMBIENT_TEMPERATURE = 13;
  public static final int TYPE_GAS_RESISTANCE = 0x10001;

  int type;
  String name;
  String vendor;
  float maxRange;
  float resolution;
  int minDelay;

  Sensor() {}

  public int getType() {
    return type;
  }

  public String getName() {
    return name;
  }

  public String getVendor() {
    return vendor;
  }

  public float getMaximumRange() {
    return maxRange;
  }

  public float getResolution() {
    return resolution;
  }

  public int getMinDelay() {
    return minDelay;
  }
}
