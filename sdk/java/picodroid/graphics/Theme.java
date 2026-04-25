package picodroid.graphics;

/**
 * App-wide color palette. Static fields apps read at view-construction time so visual changes are
 * one place to edit rather than scattered across every {@code setBackgroundColor} call.
 *
 * <p>Apps customise by assigning to these fields before any UI is built — e.g. inside {@code
 * Application.onCreate}:
 *
 * <pre>{@code
 * Theme.colorPrimary = Color.argb(255, 80, 180, 120);
 * Theme.colorBackground = Color.argb(255, 24, 24, 28);
 * }</pre>
 *
 * v1 caveats:
 *
 * <ul>
 *   <li>Static fields, not instance — picodroid is single-app, so per-Activity themes don't apply.
 *   <li>No automatic cascading: views still need to read these values explicitly. A future
 *       follow-up could add an attribute-resolver that walks the parent chain.
 *   <li>No light/dark mode switch — apps that want one toggle can do {@code if (dark) {...}}.
 * </ul>
 */
public final class Theme {
  /** Primary accent (button fill, focused outlines, slider track). */
  public static int colorPrimary = Color.argb(255, 80, 120, 200);

  /** Color of text/icons that sit on top of {@link #colorPrimary}. */
  public static int colorOnPrimary = Color.argb(255, 255, 255, 255);

  /** Page background. */
  public static int colorBackground = Color.argb(255, 18, 18, 24);

  /** Card / surface background — slightly lighter than {@link #colorBackground}. */
  public static int colorSurface = Color.argb(255, 32, 32, 40);

  /** Primary body text on background. */
  public static int colorText = Color.argb(255, 240, 240, 240);

  /** Secondary / muted body text. */
  public static int colorTextSecondary = Color.argb(255, 170, 170, 180);

  /** Subtle separator / divider line. */
  public static int colorOutline = Color.argb(255, 70, 70, 84);

  private Theme() {}
}
