package benchmark;

public class FastCounter extends Counter {
  public FastCounter() {
    super();
  }

  public int increment() {
    count = count + 2;
    return count;
  }
}
