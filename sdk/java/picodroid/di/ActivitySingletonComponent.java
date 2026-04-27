package picodroid.di;

public class ActivitySingletonComponent {
  private final ApplicationComponent app;

  protected ActivitySingletonComponent() {
    this.app = ApplicationComponent.current();
  }

  protected final ApplicationComponent app() {
    return app;
  }
}
