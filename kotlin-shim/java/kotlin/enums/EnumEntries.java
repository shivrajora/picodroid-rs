// SPDX-License-Identifier: GPL-3.0-only
package kotlin.enums;

import java.util.List;

/** The type of every Kotlin enum's {@code entries}: a read-only list of its constants. */
public interface EnumEntries<E extends Enum<E>> extends List<E> {}
