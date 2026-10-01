// SPDX-License-Identifier: GPL-3.0-only
package picodroid.lifecycle;

/**
 * A {@link LiveData} whose value anyone holding it can set; mirrors {@code
 * androidx.lifecycle.MutableLiveData}. A {@link ViewModel} keeps one of these private and hands out
 * the {@link LiveData} view of it.
 */
public class MutableLiveData<T> extends LiveData<T> {
  public MutableLiveData(T value) {
    super(value);
  }

  public MutableLiveData() {
    super();
  }

  @Override
  public void setValue(T value) {
    super.setValue(value);
  }

  @Override
  public void postValue(T value) {
    super.postValue(value);
  }
}
