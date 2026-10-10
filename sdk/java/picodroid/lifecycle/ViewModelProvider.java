// SPDX-License-Identifier: GPL-3.0-only
package picodroid.lifecycle;

/**
 * Gets a {@link ViewModel} from its owner, creating it the first time; mirrors {@code
 * androidx.lifecycle.ViewModelProvider}:
 *
 * <pre>{@code
 * UsageViewModel model = new ViewModelProvider(requireActivity()).get(UsageViewModel.class);
 * }</pre>
 *
 * <p>Every provider over the same owner returns the same instance, which is how an Activity and its
 * fragments share one. A ViewModel is made by a {@link Factory}: the one given to the constructor,
 * else the owner's {@link HasDefaultViewModelProviderFactory#getDefaultViewModelProviderFactory},
 * which for an Activity is {@link NewInstanceFactory}, {@code modelClass.newInstance()}, so a
 * ViewModel with a public no-argument constructor needs no factory, as on Android. One that takes
 * arguments gets a factory of its own, overriding {@code getDefaultViewModelProviderFactory} or
 * passed to the constructor.
 */
public class ViewModelProvider {
  /** Makes ViewModels; mirrors {@code ViewModelProvider.Factory}. */
  public interface Factory {
    /** A new instance of {@code modelClass}. */
    <T extends ViewModel> T create(Class<T> modelClass);
  }

  /**
   * The default factory, mirroring {@code ViewModelProvider.NewInstanceFactory}: {@code
   * modelClass.newInstance()}, so a ViewModel with a public no-argument constructor needs no
   * factory at all. A class without one throws {@code RuntimeException}, as on Android.
   */
  public static class NewInstanceFactory implements Factory {
    private static NewInstanceFactory sInstance;

    public NewInstanceFactory() {}

    public static NewInstanceFactory getInstance() {
      if (sInstance == null) {
        sInstance = new NewInstanceFactory();
      }
      return sInstance;
    }

    @Override
    public <T extends ViewModel> T create(Class<T> modelClass) {
      try {
        return modelClass.newInstance();
      } catch (InstantiationException | IllegalAccessException e) {
        throw new RuntimeException("Cannot create an instance of " + modelClass.getName());
      }
    }
  }

  private final ViewModelStore mStore;
  private final Factory mFactory;

  /**
   * ViewModels of {@code owner}, made by its default factory ({@link
   * HasDefaultViewModelProviderFactory}), else by {@link NewInstanceFactory}.
   */
  public ViewModelProvider(ViewModelStoreOwner owner) {
    this(
        owner,
        owner instanceof HasDefaultViewModelProviderFactory
            ? ((HasDefaultViewModelProviderFactory) owner).getDefaultViewModelProviderFactory()
            : NewInstanceFactory.getInstance());
  }

  /** ViewModels of {@code owner}, made by {@code factory} (null: {@link NewInstanceFactory}). */
  public ViewModelProvider(ViewModelStoreOwner owner, Factory factory) {
    mStore = owner.getViewModelStore();
    mFactory = factory != null ? factory : NewInstanceFactory.getInstance();
  }

  /** The owner's {@code modelClass} instance, created on the first call. */
  public <T extends ViewModel> T get(Class<T> modelClass) {
    return get(modelClass.getName(), modelClass);
  }

  /** As {@link #get(Class)}, under {@code key}: for several instances of one class. */
  @SuppressWarnings("unchecked")
  public <T extends ViewModel> T get(String key, Class<T> modelClass) {
    ViewModel model = mStore.get(key);
    if (model == null) {
      model = mFactory.create(modelClass);
      mStore.put(key, model);
    }
    return (T) model;
  }
}
