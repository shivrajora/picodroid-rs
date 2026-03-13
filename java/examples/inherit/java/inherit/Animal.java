package inherit;

public class Animal {
    int sound; // 0=generic, 1=bark, 2=meow

    public Animal() {
        this.sound = 0;
    }

    public int getSound() {
        return sound;
    }

    public int speak() {
        return sound;
    }
}
