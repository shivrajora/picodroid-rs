package inherit;

public class Dog extends Animal {
  int tricks;

  public Dog() {
    super();
    this.sound = 1; // bark
    this.tricks = 3;
  }

  @Override
  public int speak() {
    return sound * 10 + tricks;
  }
}
