// SPDX-License-Identifier: GPL-3.0-only
package picodroid.lifecycle;

import java.util.function.Function;

/**
 * Derived {@link LiveData}, mirroring {@code androidx.lifecycle.Transformations}: {@link #map}
 * applies a function to each value, {@link #switchMap} follows whichever LiveData the function
 * picks. Both are {@link MediatorLiveData}s, so the source is observed only while the result is.
 *
 * <pre>{@code
 * LiveData<String> label = Transformations.map(model.count(), n -> n + " items");
 * }</pre>
 */
public final class Transformations {
  private Transformations() {}

  /** A LiveData of {@code mapFunction} applied to each value of {@code source}. */
  public static <X, Y> LiveData<Y> map(LiveData<X> source, Function<X, Y> mapFunction) {
    MediatorLiveData<Y> result = new MediatorLiveData<>();
    result.addSource(source, x -> result.setValue(mapFunction.apply(x)));
    return result;
  }

  /**
   * A LiveData that follows the LiveData {@code switchMapFunction} returns for the latest value of
   * {@code source}; a null return follows nothing until the next value.
   */
  public static <X, Y> LiveData<Y> switchMap(
      LiveData<X> source, Function<X, LiveData<Y>> switchMapFunction) {
    MediatorLiveData<Y> result = new MediatorLiveData<>();
    result.addSource(
        source,
        new Observer<X>() {
          private LiveData<Y> mSource;

          @Override
          public void onChanged(X x) {
            LiveData<Y> next = switchMapFunction.apply(x);
            if (mSource == next) {
              return;
            }
            if (mSource != null) {
              result.removeSource(mSource);
            }
            mSource = next;
            if (next != null) {
              result.addSource(next, result::setValue);
            }
          }
        });
    return result;
  }
}
