//! What the webview gets for an open book.

use crate::library::load::LoadedBook;
use crate::library::BookError;
use serde::Serialize;

/// One page of a [`BookView`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PageView {
    /// Canonical absolute path of the display image, for `convertFileSrc`.
    pub path: String,
    /// Width in pixels.
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
}

/// An open book: `{ id, title, root, lastReadPage, pages: [{ path, width, height }] }`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BookView {
    /// The book id.
    pub id: String,
    /// The book title.
    pub title: String,
    /// Canonical absolute path of the package; pass it back to `save_progress`.
    pub root: String,
    /// Zero-based page to open on.
    pub last_read_page: u32,
    /// Every page in reading order.
    pub pages: Vec<PageView>,
}

fn invalid(reason: impl Into<String>) -> BookError {
    BookError::InvalidPackage {
        reason: reason.into(),
    }
}

/// Build the view of `book`. Each page path is the verified canonical path from
/// `LoadedBook::page_path`, never a fresh join of `meta.pages[i].file`.
pub fn book_view(book: &LoadedBook) -> Result<BookView, BookError> {
    // `dunce::simplified` drops Windows' `\\?\` prefix where that is safe, so the webview
    // gets ordinary `C:\...` paths. It changes nothing on macOS and Linux.
    let root = dunce::simplified(&book.root)
        .to_str()
        .ok_or_else(|| invalid("the book path is not valid UTF-8"))?
        .to_owned();
    let mut pages = Vec::with_capacity(book.meta.pages.len());
    for (i, page) in book.meta.pages.iter().enumerate() {
        let path = book
            .page_path(i)
            .and_then(|p| dunce::simplified(p).to_str())
            .ok_or_else(|| invalid(format!("page {i} has no usable path")))?;
        pages.push(PageView {
            path: path.to_owned(),
            width: page.width,
            height: page.height,
        });
    }
    Ok(BookView {
        id: book.meta.id.clone(),
        title: book.meta.title.clone(),
        root,
        last_read_page: book.meta.last_read_page,
        pages,
    })
}
