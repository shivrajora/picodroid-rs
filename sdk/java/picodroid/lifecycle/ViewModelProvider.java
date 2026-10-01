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
 * fragments share one. There is no reflection on this runtime, so a ViewModel is always made by a
 * {@link Factory}: the one given to the constructor, else the owner's {@link
 * HasDefaultViewModelProviderFactory#getDefaultViewModelProviderFactory}, which an Activity
 * overrides:
 *
 * <pre>{@code
 * @Override
 * public ViewModelProvider.Factory getDefaultViewModelProviderFactory() {
 *   return new ViewModelProvider.Factory() {
 *     @Override
 *     @SuppressWarnings("unchecked")
 *     public <T extends ViewModel> T create(Class<T> modelClass) {
 *       return (T) new UsageViewModel();
 *     }
 *   };
 * }
 * }</pre>
 */
public class ViewModelProvider {
  /** Makes ViewModels; mirrors {@code ViewModelProvider.Factory}. */
  public interface Factory {
    /** A new instance of {@code modelClass}. */
    <T extends ViewModel> T create(Class<T> modelClass);
  }

  private final ViewModelStore mStore;
  private final Factory mFactory;

  /** ViewModels of {@code owner}, made by its default factory. */
  public ViewModelProvider(ViewModelStoreOwner owner) {
    this(
        owner,
        owner instanceof HasDefaultViewModelProviderFactory
            ? ((HasDefaultViewModelProviderFactory) owner).getDefaultViewModelProviderFactory()
            : null);
  }

  /** ViewModels of {@code owner}, made by {@code factory}. */
  public ViewModelProvider(ViewModelStoreOwner owner, Factory factory) {
    mStore = owner.getViewModelStore();
    mFactory = factory;
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
      if (mFactory == null) {
        throw new IllegalStateException(
            "No ViewModelProvider.Factory for "
                + modelClass.getName()
                + ": pass one, or override getDefaultViewModelProviderFactory");
      }
      model = mFactory.create(modelClass);
      mStore.put(key, model);
    }
    return (T) model;
  }
}
