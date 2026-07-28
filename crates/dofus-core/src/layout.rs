//! Arranging tabs into a grid.
//!
//! A single row does not scale: eight accounts make a bar over a thousand
//! pixels wide. Instead tabs are laid out in a grid that wraps after a
//! configurable number of items, so the bar stays compact whatever the account
//! count.
//!
//! Pure arithmetic, kept out of the GUI crate so it can be tested directly.

use serde::{Deserialize, Serialize};

/// Items placed before the layout wraps to a new column or row.
pub const DEFAULT_WRAP_AT: u32 = 4;

/// Direction tabs are filled in before wrapping.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Orientation {
    /// Fill downwards, then start a new column. Keeps the bar narrow.
    #[default]
    Vertical,
    /// Fill rightwards, then start a new row. Keeps the bar short.
    Horizontal,
}

/// Dimensions of the resulting grid.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Grid {
    pub columns: u32,
    pub rows: u32,
}

impl Grid {
    pub fn is_empty(&self) -> bool {
        self.columns == 0 || self.rows == 0
    }
}

/// Computes the grid needed to show `count` tabs.
pub fn grid(count: usize, orientation: Orientation, wrap_at: u32) -> Grid {
    let count = count as u32;
    if count == 0 {
        return Grid {
            columns: 0,
            rows: 0,
        };
    }

    let wrap = effective_wrap(wrap_at);
    let filled = count.min(wrap);
    let wrapped = count.div_ceil(wrap);

    match orientation {
        Orientation::Vertical => Grid {
            columns: wrapped,
            rows: filled,
        },
        Orientation::Horizontal => Grid {
            columns: filled,
            rows: wrapped,
        },
    }
}

/// Index of the tab shown at `(column, row)`, if any.
pub fn index_at(
    column: u32,
    row: u32,
    count: usize,
    orientation: Orientation,
    wrap_at: u32,
) -> Option<usize> {
    let wrap = effective_wrap(wrap_at);
    let index = match orientation {
        Orientation::Vertical => column * wrap + row,
        Orientation::Horizontal => row * wrap + column,
    } as usize;

    (index < count).then_some(index)
}

/// Guards against a config that would divide by zero.
fn effective_wrap(wrap_at: u32) -> u32 {
    wrap_at.max(1)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Reads the grid back into rows of indices, the way the bar renders it.
    fn render(count: usize, orientation: Orientation, wrap_at: u32) -> Vec<Vec<Option<usize>>> {
        let grid = grid(count, orientation, wrap_at);
        (0..grid.rows)
            .map(|row| {
                (0..grid.columns)
                    .map(|column| index_at(column, row, count, orientation, wrap_at))
                    .collect()
            })
            .collect()
    }

    #[test]
    fn two_tabs_stack_vertically() {
        assert_eq!(
            grid(2, Orientation::Vertical, 4),
            Grid {
                columns: 1,
                rows: 2
            }
        );
        assert_eq!(
            render(2, Orientation::Vertical, 4),
            vec![vec![Some(0)], vec![Some(1)]]
        );
    }

    #[test]
    fn a_full_column_does_not_wrap_yet() {
        assert_eq!(
            grid(4, Orientation::Vertical, 4),
            Grid {
                columns: 1,
                rows: 4
            }
        );
    }

    #[test]
    fn the_fifth_tab_starts_a_second_column() {
        assert_eq!(
            grid(5, Orientation::Vertical, 4),
            Grid {
                columns: 2,
                rows: 4
            }
        );
        assert_eq!(
            render(5, Orientation::Vertical, 4),
            vec![
                vec![Some(0), Some(4)],
                vec![Some(1), None],
                vec![Some(2), None],
                vec![Some(3), None],
            ]
        );
    }

    #[test]
    fn eight_tabs_fill_two_columns_of_four() {
        assert_eq!(
            grid(8, Orientation::Vertical, 4),
            Grid {
                columns: 2,
                rows: 4
            }
        );
        assert_eq!(
            render(8, Orientation::Vertical, 4),
            vec![
                vec![Some(0), Some(4)],
                vec![Some(1), Some(5)],
                vec![Some(2), Some(6)],
                vec![Some(3), Some(7)],
            ]
        );
    }

    #[test]
    fn horizontal_fills_rows_instead() {
        assert_eq!(
            grid(8, Orientation::Horizontal, 4),
            Grid {
                columns: 4,
                rows: 2
            }
        );
        assert_eq!(
            render(8, Orientation::Horizontal, 4),
            vec![
                vec![Some(0), Some(1), Some(2), Some(3)],
                vec![Some(4), Some(5), Some(6), Some(7)],
            ]
        );
    }

    #[test]
    fn a_single_row_is_still_possible() {
        assert_eq!(
            grid(8, Orientation::Horizontal, 99),
            Grid {
                columns: 8,
                rows: 1
            }
        );
    }

    #[test]
    fn no_tabs_means_no_grid() {
        assert!(grid(0, Orientation::Vertical, 4).is_empty());
        assert_eq!(index_at(0, 0, 0, Orientation::Vertical, 4), None);
    }

    #[test]
    fn a_zero_wrap_does_not_divide_by_zero() {
        assert_eq!(
            grid(3, Orientation::Vertical, 0),
            Grid {
                columns: 3,
                rows: 1
            }
        );
    }

    #[test]
    fn every_tab_appears_exactly_once() {
        for orientation in [Orientation::Vertical, Orientation::Horizontal] {
            for count in 1..=8usize {
                let mut seen: Vec<usize> = render(count, orientation, 4)
                    .into_iter()
                    .flatten()
                    .flatten()
                    .collect();
                seen.sort_unstable();
                assert_eq!(
                    seen,
                    (0..count).collect::<Vec<_>>(),
                    "{orientation:?} {count}"
                );
            }
        }
    }
}
