package picodroid.di;

public class ApplicationComponent {
  private static ApplicationComponent INSTANCE;

  protected ApplicationComponent() {
    INSTANCE = this;
  }

  public static ApplicationComponent current() {
    if (INSTANCE == null) {
      throw new IllegalStateException(
          "ApplicationComponent.current() called before any was constructed; "
              + "call new MyAppComponent() in Application.onCreate()");
    }
    return INSTANCE;
  }
}
