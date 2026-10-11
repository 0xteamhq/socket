//! The blocks of a page as they nest.

use super::Block;

/// Why the blocks inside a block were not read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Cut {
    /// They are nested deeper than was asked for.
    Depth,
    /// The requests allowed for the page were spent.
    Requests,
    /// Notion would not list them: a block synced from a page that was not
    /// shared, for one.
    Refused,
}

impl Cut {
    /// The line that stands in the Markdown where blocks were not read.
    pub(super) fn note(self) -> &'static str {
        match self {
            Self::Depth => "[not read: blocks nested deeper than the depth limit]",
            Self::Requests => "[not read: more blocks, the request limit was reached]",
            Self::Refused => "[not read: Notion did not give the blocks inside this one]",
        }
    }
}

#[derive(Debug)]
pub(super) struct Node {
    /// `None` for the page itself.
    pub(super) block: Option<Block>,
    pub(super) inside: Vec<usize>,
    pub(super) cut: Option<Cut>,
}

/// The blocks of a page as they nest.
#[derive(Debug)]
pub(crate) struct Tree {
    pub(super) nodes: Vec<Node>,
}

impl Tree {
    /// The page itself, which every top-level block is inside of.
    pub(crate) const PAGE: usize = 0;

    pub(crate) fn new() -> Self {
        let page = Node {
            block: None,
            inside: Vec::new(),
            cut: None,
        };
        Self { nodes: vec![page] }
    }

    /// Puts `block` last inside `parent` and returns its place.
    pub(crate) fn add(&mut self, parent: usize, block: Block) -> usize {
        let place = self.nodes.len();
        self.nodes.push(Node {
            block: Some(block),
            inside: Vec::new(),
            cut: None,
        });
        self.nodes[parent].inside.push(place);
        place
    }

    /// Records that not all the blocks inside `node` were read.
    pub(crate) fn cut(&mut self, node: usize, why: Cut) {
        self.nodes[node].cut = Some(why);
    }

    pub(crate) fn was_cut(&self, why: Cut) -> bool {
        self.nodes.iter().any(|node| node.cut == Some(why))
    }

    /// How many blocks were read.
    pub(crate) fn blocks(&self) -> usize {
        self.nodes.len() - 1
    }
}
