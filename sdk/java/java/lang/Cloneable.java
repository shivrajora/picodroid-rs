// SPDX-License-Identifier: GPL-3.0-only
package java.lang;

/**
 * Mirrors {@code java.lang.Cloneable}: the marker interface for {@link Object#clone()}.
 *
 * <p>Picodroid divergence (see the compatibility matrix): the JVM does not enforce the marker —
 * {@code clone()} on a class that does not implement Cloneable performs the shallow copy instead of
 * throwing {@code CloneNotSupportedException}, because native dispatch has no view of the interface
 * table. Implement it anyway for source fidelity with Android/JDK code.
 */
public interface Cloneable {}
