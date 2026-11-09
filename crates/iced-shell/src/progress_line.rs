//! The reading position as one value, shared by the Continue Reading card and
//! the progress line along the bottom of the reading area.

use super::*;

/// How far through the open book or PDF the reader is.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct Progress {
    /// The current page, from 1.
    pub(super) page: u32,
    pub(super) total: u32,
    /// From 0 at the start of the first page to 1 at the end of the last.
    pub(super) fraction: f32,
}

impl Progress {
    pub(super) fn label(&self) -> String {
        format!(
            "Page {} of {} · {}%",
            self.page,
            self.total,
            shelf::percent(self.fraction)
        )
    }
}

/// The page a press at `fraction` of the line's width lands on.
pub(super) fn page_at(fraction: f32, total: u32) -> u32 {
    ((fraction * total as f32).ceil() as u32).clamp(1, total.max(1))
}

impl Reader {
    /// `None` without a document, and while a book is still being paginated.
    pub(super) fn progress(&self) -> Option<Progress> {
        let (page, within, total) = if let Some(pdf) = &self.pdf {
            let position = pdf.position();
            (
                position.page,
                position.within,
                pdf.document().pages.len() as u32,
            )
        } else {
            let page = book_pages::current(self.pages(), self.offset)?;
            let within = ((self.offset - page.top) / page.height).clamp(0.0, 1.0);
            (page.number, within, self.page_total() as u32)
        };
        (total > 0).then(|| Progress {
            page: page + 1,
            total,
            fraction: ((page as f32 + within) / total as f32).clamp(0.0, 1.0),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn click_fraction_maps_to_page() {
        assert_eq!(page_at(0.5, 14), 7);
        assert_eq!(page_at(1.0, 14), 14);
        assert_eq!(page_at(0.0, 14), 1);
        assert_eq!(page_at(0.51, 14), 8);
    }
}
