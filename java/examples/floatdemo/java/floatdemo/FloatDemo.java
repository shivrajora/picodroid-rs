package floatdemo;

import picodroid.util.Log;

public class FloatDemo {
  public static void main(String[] args) {
    float x = 3.0f;
    float y = 2.0f;
    float z = x * y; // 6.0
    int result = (int) z; // 6 (f2i)
    Log.i("FloatDemo", "Float math works!");
  }
}
