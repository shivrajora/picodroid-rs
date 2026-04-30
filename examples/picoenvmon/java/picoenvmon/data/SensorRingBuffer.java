package picoenvmon.data;

/** Fixed-capacity circular buffer of float samples. Overwrites oldest on full. */
public class SensorRingBuffer {
  private final float[] data;
  private int head; // next write index
  private int size; // number of valid samples (≤ capacity)

  public SensorRingBuffer(int capacity) {
    this.data = new float[capacity];
  }

  public void add(float sample) {
    data[head] = sample;
    head = (head + 1) % data.length;
    if (size < data.length) {
      size++;
    }
  }

  public int size() {
    return size;
  }

  public int capacity() {
    return data.length;
  }

  /** Copy oldest-first samples into {@code out}. Returns the number of samples written. */
  public int snapshot(float[] out) {
    int n = size;
    int start = (head - size + data.length) % data.length;
    for (int i = 0; i < n && i < out.length; i++) {
      out[i] = data[(start + i) % data.length];
    }
    return n;
  }
}
