// SPDX-License-Identifier: GPL-3.0-only
package picoenvmon.data;

/**
 * Fixed-capacity circular buffer of float samples plus a parallel epoch-second timestamp per sample
 * (0 = wall clock not yet NTP-synced when captured). Overwrites oldest on full. Primitive arrays
 * only, allocated once — zero per-sample churn.
 */
public class SensorRingBuffer {
  private final float[] data;
  private final int[] ts; // epoch seconds; 0 = unknown (int is fine until 2038)
  private int head; // next write index
  private int size; // number of valid samples (≤ capacity)

  public SensorRingBuffer(int capacity) {
    this.data = new float[capacity];
    this.ts = new int[capacity];
  }

  public void add(float sample) {
    add(sample, 0);
  }

  public void add(float sample, int epochSec) {
    data[head] = sample;
    ts[head] = epochSec;
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

  /** As {@link #snapshot(float[])}, also copying each sample's epoch-second timestamp. */
  public int snapshot(float[] out, int[] tsOut) {
    int n = snapshot(out);
    int start = (head - size + data.length) % data.length;
    for (int i = 0; i < n && i < tsOut.length; i++) {
      tsOut[i] = ts[(start + i) % data.length];
    }
    return n;
  }
}
