// SPDX-License-Identifier: GPL-3.0-only
//! The arithmetic of a panel that scrolls its own frame memory, kept free of
//! LVGL so it runs under `cargo test` (docs/designs/scroll-performance-2026-09.md
//! S4). [`super::hw_scroll`] owns the state and the events; this file owns
//! the three sums it needs.
//!
//! The panel model, which is the ST7796's and the simulator's alike: display
//! rows `[top, top + rows)` show memory lines from the same band, rotated so
//! that display row `top + i` shows memory line `top + ((i + origin) mod
//! rows)`. Rows outside the band show their own memory line. `origin == 0`
//! is the identity, which is also what an undefined band looks like.

/// The rows the panel rotates: `[top, top + rows)`, in display coordinates.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Region {
    pub top: i32,
    pub rows: i32,
}

/// A run of display rows `y1..=y2` whose pixels live at memory lines
/// starting at `mem_y1` — contiguous, because a run stops at the band's wrap.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct Seg {
    pub y1: i32,
    pub y2: i32,
    pub mem_y1: i32,
}

/// The most segments one flushed area can split into: a fixed run above the
/// band, the band up to its wrap, the band after the wrap, a fixed run below.
pub const MAX_SEGMENTS: usize = 4;

/// The origin after the content moved by `dy` display rows (positive is
/// down). Content moving up by 47 means display row `r` must now show what
/// row `r + 47` showed, so the origin advances by 47.
pub fn advance(origin: i32, rows: i32, dy: i32) -> i32 {
    (origin - dy).rem_euclid(rows)
}

/// The display rows that show stale memory after the content moved by `dy`:
/// the bottom `|dy|` rows of the band when content moved up, the top `dy`
/// when it moved down. Inclusive, clamped to the band.
pub fn exposed_rows(region: Region, dy: i32) -> (i32, i32) {
    let n = dy.abs().min(region.rows);
    let bottom = region.top + region.rows - 1;
    if dy < 0 {
        (bottom - n + 1, bottom)
    } else {
        (region.top, region.top + n - 1)
    }
}

/// Split display rows `y1..=y2` into memory-contiguous runs under the
/// current rotation. Without a band, or at the identity, the answer is the
/// one run the caller would have written anyway.
pub fn segments(
    y1: i32,
    y2: i32,
    region: Option<Region>,
    origin: i32,
) -> ([Seg; MAX_SEGMENTS], usize) {
    let mut out = [Seg::default(); MAX_SEGMENTS];
    let mut n = 0;
    let Some(r) = region.filter(|_| origin != 0) else {
        out[0] = Seg { y1, y2, mem_y1: y1 };
        return (out, 1);
    };
    let band_top = r.top;
    let band_bottom = r.top + r.rows - 1;

    if y1 < band_top {
        let end = y2.min(band_top - 1);
        out[n] = Seg {
            y1,
            y2: end,
            mem_y1: y1,
        };
        n += 1;
    }

    let mut y = y1.max(band_top);
    let last = y2.min(band_bottom);
    while y <= last {
        let offset = (y - band_top + origin).rem_euclid(r.rows);
        let run = (r.rows - offset).min(last - y + 1);
        out[n] = Seg {
            y1: y,
            y2: y + run - 1,
            mem_y1: band_top + offset,
        };
        n += 1;
        y += run;
    }

    if y2 > band_bottom {
        let start = y1.max(band_bottom + 1);
        out[n] = Seg {
            y1: start,
            y2,
            mem_y1: start,
        };
        n += 1;
    }
    (out, n)
}

#[cfg(test)]
mod tests {
    use super::*;

    const BAND: Region = Region { top: 44, rows: 436 };

    fn segs(y1: i32, y2: i32, region: Option<Region>, origin: i32) -> Vec<Seg> {
        let (arr, n) = segments(y1, y2, region, origin);
        arr[..n].to_vec()
    }

    #[test]
    fn identity_without_a_band_or_at_origin_zero() {
        let one = vec![Seg {
            y1: 10,
            y2: 129,
            mem_y1: 10,
        }];
        assert_eq!(segs(10, 129, None, 0), one);
        assert_eq!(segs(10, 129, None, 99), one);
        assert_eq!(segs(10, 129, Some(BAND), 0), one);
    }

    #[test]
    fn rows_above_and_below_the_band_stay_where_they_are() {
        // A band of 120 rows starting at the very top: the header above the
        // scroller is fixed, the first rows of the scroller are rotated.
        let s = segs(0, 119, Some(BAND), 47);
        assert_eq!(
            s,
            vec![
                Seg {
                    y1: 0,
                    y2: 43,
                    mem_y1: 0
                },
                Seg {
                    y1: 44,
                    y2: 119,
                    mem_y1: 44 + 47
                },
            ]
        );
        // The bottom fixed rows of a shorter band.
        let short = Region { top: 44, rows: 100 };
        let s = segs(120, 239, Some(short), 30);
        assert_eq!(
            s,
            vec![
                Seg {
                    y1: 120,
                    y2: 143,
                    mem_y1: 44 + ((120 - 44 + 30) % 100)
                },
                Seg {
                    y1: 144,
                    y2: 239,
                    mem_y1: 144
                },
            ]
        );
    }

    #[test]
    fn a_run_splits_at_the_wrap_and_nowhere_else() {
        // Origin 400 of 436: display row 44 shows line 444; rows 44..79 run
        // to line 479, then the band wraps to line 44.
        let s = segs(44, 163, Some(BAND), 400);
        assert_eq!(
            s,
            vec![
                Seg {
                    y1: 44,
                    y2: 79,
                    mem_y1: 444
                },
                Seg {
                    y1: 80,
                    y2: 163,
                    mem_y1: 44
                },
            ]
        );
        // Every segment maps rows one-to-one and the runs cover the input.
        let total: i32 = s.iter().map(|g| g.y2 - g.y1 + 1).sum();
        assert_eq!(total, 120);
        for g in &s {
            assert!(g.mem_y1 >= BAND.top);
            assert!(g.mem_y1 + (g.y2 - g.y1) <= BAND.top + BAND.rows - 1);
        }
    }

    #[test]
    fn the_last_band_of_a_full_repaint_reaches_the_panels_last_line() {
        let s = segs(360, 479, Some(BAND), 1);
        assert_eq!(
            s,
            vec![
                Seg {
                    y1: 360,
                    y2: 478,
                    mem_y1: 361
                },
                Seg {
                    y1: 479,
                    y2: 479,
                    mem_y1: 44
                },
            ]
        );
    }

    #[test]
    fn the_origin_follows_the_content_and_wraps() {
        // Content moves up 47 rows: the origin advances 47.
        assert_eq!(advance(0, 436, -47), 47);
        // Then back down 60: 13 short of the start, i.e. wrapped.
        assert_eq!(advance(47, 436, 60), 436 - 13);
        assert_eq!(advance(430, 436, -10), 4);
        assert_eq!(advance(5, 436, 0), 5);
    }

    #[test]
    fn exposed_rows_are_the_trailing_edge_of_the_motion() {
        // Up: the bottom 47 rows of the band are stale.
        assert_eq!(exposed_rows(BAND, -47), (479 - 46, 479));
        // Down: the top 12.
        assert_eq!(exposed_rows(BAND, 12), (44, 55));
        // A step larger than the band exposes the whole band.
        assert_eq!(exposed_rows(BAND, -1000), (44, 479));
        assert_eq!(exposed_rows(BAND, 1000), (44, 479));
    }

    #[test]
    fn exposed_rows_after_a_step_are_exactly_what_the_rotation_moved_in() {
        // Property: after moving content by dy, the rows `exposed_rows`
        // names are the ones whose memory line, under the new origin, was
        // displayed at a row outside the band under the old origin (i.e. no
        // row inside the band ever showed it — it scrolled in from nowhere).
        for &dy in &[-1, -47, -435, 1, 30, 435] {
            let old = 100;
            let new = advance(old, BAND.rows, dy);
            let (e1, e2) = exposed_rows(BAND, dy);
            for y in BAND.top..BAND.top + BAND.rows {
                let line = BAND.top + (y - BAND.top + new).rem_euclid(BAND.rows);
                // Which display row showed `line` before the step?
                let shown_at = BAND.top + (line - BAND.top - old).rem_euclid(BAND.rows);
                let moved_off =
                    shown_at + dy < BAND.top || shown_at + dy > BAND.top + BAND.rows - 1;
                let flagged = (e1..=e2).contains(&y);
                assert_eq!(flagged, moved_off, "dy {dy} row {y}");
            }
        }
    }
}
