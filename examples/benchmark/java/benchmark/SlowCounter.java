package benchmark;

public class SlowCounter extends Counter {
  public SlowCounter() {
    super();
  }

  public int increment() {
    count = count + 1;
    return count;
  }
}
