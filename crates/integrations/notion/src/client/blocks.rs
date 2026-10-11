//! Blocks: the pieces a page's content is made of.

use serde_json::{Map, Value, json};
use socketkit_core::{ErrorKind, Page, RawRequest, Result};

use super::{Api, patch};
use crate::models::{AppendBlocks, Block, Paging, PositionKind, UpdateBlock};

/// The most blocks Notion takes in one request.
const MOST_BLOCKS: usize = 100;

/// Blocks: the pieces a page's content is made of.
#[derive(Debug, Clone, Copy)]
pub struct Blocks<'a>(pub(crate) Api<'a>);

impl Blocks<'_> {
    /// Gets one block.
    pub async fn get(&self, block: &str) -> Result<Block> {
        let body = self.0.send(RawRequest::get(self.path(block)?)).await?;
        self.0.object(body, "block")
    }

    /// Lists the blocks directly inside a block or a page, in order. Each
    /// says in `has_children` whether it holds blocks of its own.
    pub async fn children(&self, block: &str, paging: Paging) -> Result<Page<Block>> {
        self.0.children(block, &paging).await
    }

    /// Adds blocks inside a block or a page, and returns them as Notion made them.
    pub async fn append(&self, block: &str, blocks: AppendBlocks) -> Result<Page<Block>> {
        let invalid = |message: &str| self.0.error(ErrorKind::InvalidInput, message);
        if !(1..=MOST_BLOCKS).contains(&blocks.children.len()) {
            return Err(invalid("`children` holds from 1 to 100 blocks"));
        }
        let mut body = Map::new();
        body.insert("children".to_owned(), json!(blocks.children));
        if let Some(position) = &blocks.position {
            match (position.kind, &position.after_block) {
                (PositionKind::AfterBlock, None) => {
                    return Err(invalid(
                        "a `position` of `after_block` names the block in `after_block.id`",
                    ));
                }
                (PositionKind::Start | PositionKind::End, Some(_)) => {
                    return Err(invalid("`after_block` goes with a `position` of `after_block` only"));
                }
                _ => body.insert("position".to_owned(), json!(position)),
            };
        }
        let path = format!("{}/children", self.path(block)?);
        let added = self.0.send(patch(path, body.into())).await?;
        self.0.list(added, "blocks")
    }

    /// Changes what a block holds: its text, whether a to-do is checked, a
    /// code block's language. Whatever is given replaces what was there.
    pub async fn update(&self, block: &str, changes: UpdateBlock) -> Result<Block> {
        let invalid = |message: &str| self.0.error(ErrorKind::InvalidInput, message);
        let mut content = changes.content.into_iter();
        let (Some((kind, held)), None) = (content.next(), content.next()) else {
            return Err(invalid(
                "`content` names one kind of block and what to set in it, such as `{ \"to_do\": { \"checked\": true } }`",
            ));
        };
        // Notion takes these beside a block's content, and they are not a
        // change to it: they put the block in the trash or take it out.
        if matches!(kind.as_str(), "in_trash" | "archived") {
            return Err(invalid("a block is moved to the trash with `blocks.delete`"));
        }
        if !held.is_object() {
            return Err(invalid("what to set in the block is an object"));
        }
        let body = Value::Object(Map::from_iter([(kind, held)]));
        let changed = self.0.send(patch(self.path(block)?, body)).await?;
        self.0.object(changed, "block")
    }

    /// Moves a block to the trash, with everything inside it. Given the id
    /// of a page, it moves the page there.
    pub async fn delete(&self, block: &str) -> Result<Block> {
        let body = self.0.send(RawRequest::new("DELETE", self.path(block)?)).await?;
        self.0.object(body, "block")
    }

    fn path(&self, block: &str) -> Result<String> {
        Ok(format!("blocks/{}", self.0.id("a block id", block)?))
    }
}
