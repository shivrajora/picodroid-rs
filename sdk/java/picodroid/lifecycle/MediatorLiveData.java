// SPDX-License-Identifier: GPL-3.0-only
package picodroid.lifecycle;

import java.util.ArrayList;

/**
 * A {@link LiveData} that follows other LiveData; mirrors {@code
 * androidx.lifecycle.MediatorLiveData}. Each {@link #addSource source} is observed only while this
 * one has an active observer, and its observer decides what to {@link #setValue}: a merge of two
 * sources, a derived value ({@link Transformations#map}), a switch between sources ({@link
 * Transformations#switchMap}).
 *
 * <pre>{@code
 * MediatorLiveData<String> both = new MediatorLiveData<>();
 * both.addSource(first, v -> both.setValue("first: " + v));
 * both.addSource(second, v -> both.setValue("second: " + v));
 * }</pre>
 */
public class MediatorLiveData<T> extends MutableLiveData<T> {
  /** The sources, in the order added; null while there are none. */
  private ArrayList<Source<?>> mSources;

  public MediatorLiveData() {
    super();
  }

  public MediatorLiveData(T value) {
    super(value);
  }

  /**
   * Starts following {@code source}: {@code onChanged} is told each value it publishes while this
   * LiveData is active, as on Android. Adding a source twice with the same observer is a no-op;
   * with another observer it throws.
   */
  public <S> void addSource(LiveData<S> source, Observer<? super S> onChanged) {
    if (source == null) {
      throw new NullPointerException("source cannot be null");
    }
    Source<?> existing = find(source);
    if (existing != null) {
      if (existing.mObserver != onChanged) {
        throw new IllegalArgumentException(
            "This source was already added with the different observer");
      }
      return;
    }
    Source<S> added = new Source<>(source, onChanged);
    if (mSources == null) {
      mSources = new ArrayList<>();
    }
    mSources.add(added);
    if (hasActiveObservers()) {
      added.plug();
    }
  }

  /** Stops following {@code toRemove}; a LiveData that is not a source is a no-op. */
  public <S> void removeSource(LiveData<S> toRemove) {
    Source<?> source = find(toRemove);
    if (source != null) {
      mSources.remove(source);
      source.unplug();
    }
  }

  @Override
  protected void onActive() {
    ArrayList<Source<?>> sources = mSources;
    if (sources != null) {
      for (int i = 0; i < sources.size(); i++) {
        sources.get(i).plug();
      }
    }
  }

  @Override
  protected void onInactive() {
    ArrayList<Source<?>> sources = mSources;
    if (sources != null) {
      for (int i = 0; i < sources.size(); i++) {
        sources.get(i).unplug();
      }
    }
  }

  private Source<?> find(LiveData<?> liveData) {
    ArrayList<Source<?>> sources = mSources;
    if (sources != null) {
      for (int i = 0; i < sources.size(); i++) {
        Source<?> source = sources.get(i);
        if (source.mLiveData == liveData) {
          return source;
        }
      }
    }
    return null;
  }

  /**
   * One source and its observer. Observing forever while plugged keeps the source's own activity
   * out of it; the version check keeps a value from being delivered twice across an unplug and a
   * plug, as Android's {@code MediatorLiveData.Source} does.
   */
  private static final class Source<V> implements Observer<V> {
    final LiveData<V> mLiveData;
    final Observer<? super V> mObserver;
    int mVersion = START_VERSION;

    Source(LiveData<V> liveData, Observer<? super V> observer) {
      mLiveData = liveData;
      mObserver = observer;
    }

    void plug() {
      mLiveData.observeForever(this);
    }

    void unplug() {
      mLiveData.removeObserver(this);
    }

    @Override
    public void onChanged(V value) {
      int version = mLiveData.getVersion();
      if (mVersion != version) {
        mVersion = version;
        mObserver.onChanged(value);
      }
    }
  }
}
