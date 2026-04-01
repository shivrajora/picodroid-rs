package benchmark;

public class Counter implements Countable {
  int count;

  public Counter() {
    this.count = 0;
  }

  public int increment() {
    count = count + 1;
    return count;
  }

  public int count() {
    return count;
  }
}
