//! Pages: reading a page's content whole.
//!
//! This is a method of [`Pages`], whose struct and other methods are in
//! `pages.rs`.

use std::collections::VecDeque;

use socketkit_core::{ErrorKind, Result};

use super::pages::Pages;
use crate::models::{Cut, PageContent, Paging, ReadPage, Tree};

/// How deep and how long a page is read when the caller does not say, and
/// the most a caller may ask for.
const DEPTH: (u32, u32) = (10, 50);
const REQUESTS: (u32, u32) = (50, 500);

impl Pages<'_> {
    /// Reads a page's whole content as Markdown.
    ///
    /// Notion keeps a page as a tree of blocks and returns one list of them
    /// at a time, a hundred to a request, so a page takes one request for
    /// itself and one or more for each block that has others inside it. The
    /// top of the tree is read first and whole, then what is nested in it, a
    /// level at a time, so that a limit cuts off the detail and not the end.
    ///
    /// Reading stops at `max_depth` levels and after `max_requests`
    /// requests. When either cut the page short, `truncated` says so,
    /// `truncation` says which, and a line in the Markdown marks each place.
    /// The same is done for a nested block whose insides Notion will not
    /// give, such as one synced from a page that was not shared: the rest of
    /// the page is still read. A page inside the page is written as a link
    /// and not read.
    pub async fn read(&self, page: &str, options: ReadPage) -> Result<PageContent> {
        let max_depth = self.within("max_depth", options.max_depth, DEPTH)?;
        let max_requests = self.within("max_requests", options.max_requests, REQUESTS)?;
        let page = self.get(page).await?;
        let mut requests = 1;
        let mut tree = Tree::new();
        // Blocks whose insides are still to be read: where each is in the
        // tree, its id, and how deep its insides are.
        let mut waiting = VecDeque::from([(Tree::PAGE, page.id.clone(), 1)]);
        'reading: while let Some((place, id, depth)) = waiting.pop_front() {
            let mut cursor = None;
            loop {
                if requests >= max_requests {
                    tree.cut(place, Cut::Requests);
                    waiting.iter().for_each(|(unread, ..)| tree.cut(*unread, Cut::Requests));
                    break 'reading;
                }
                requests += 1;
                let paging = Paging {
                    cursor,
                    limit: Some(100),
                };
                let listed = match self.0.children(&id, &paging).await {
                    Ok(listed) => listed,
                    Err(e)
                        if place != Tree::PAGE && matches!(e.kind(), ErrorKind::NotFound | ErrorKind::AccessDenied) =>
                    {
                        tree.cut(place, Cut::Refused);
                        break;
                    }
                    Err(e) => return Err(e),
                };
                for block in listed.items {
                    // A page or a database inside the page is content of its own.
                    let nested = block.has_children && !matches!(block.kind(), "child_page" | "child_database");
                    let inner = block.id.clone();
                    let added = tree.add(place, block);
                    if nested && depth >= max_depth {
                        tree.cut(added, Cut::Depth);
                    } else if nested {
                        waiting.push_back((added, inner, depth + 1));
                    }
                }
                cursor = listed.next_cursor;
                if cursor.is_none() {
                    break;
                }
            }
        }
        let cuts = [
            (
                Cut::Depth,
                format!("blocks nested more than {max_depth} deep were not read"),
            ),
            (Cut::Requests, format!("reading stopped after {max_requests} requests")),
            (
                Cut::Refused,
                "Notion did not give the blocks inside one or more blocks".to_owned(),
            ),
        ];
        let reasons: Vec<String> = cuts
            .into_iter()
            .filter(|(cut, _)| tree.was_cut(*cut))
            .map(|(_, reason)| reason)
            .collect();
        Ok(PageContent {
            title: page.title(),
            url: page.url,
            id: page.id,
            markdown: tree.markdown(),
            truncated: !reasons.is_empty(),
            truncation: Some(reasons.join("; ")).filter(|reasons| !reasons.is_empty()),
            blocks: u32::try_from(tree.blocks()).unwrap_or(u32::MAX),
            requests,
        })
    }

    /// A limit as given, or its default, once it is known to be in range.
    fn within(&self, name: &str, given: Option<u32>, (default, most): (u32, u32)) -> Result<u32> {
        match given {
            Some(limit) if !(1..=most).contains(&limit) => Err(self
                .0
                .error(ErrorKind::InvalidInput, format!("`{name}` is from 1 to {most}"))),
            Some(limit) => Ok(limit),
            None => Ok(default),
        }
    }
}
