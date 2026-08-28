// SPDX-License-Identifier: GPL-3.0-only
package kotlin.enums;

/** Facade called from every Kotlin enum's {@code <clinit>} to build its {@code $ENTRIES}. */
public final class EnumEntriesKt {
  private EnumEntriesKt() {}

  public static <E extends Enum<E>> EnumEntries<E> enumEntries(E[] entries) {
    return new EnumEntriesList<>(entries);
  }
}
