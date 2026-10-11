//! Databases, and the data sources that hold their rows.

use serde_json::{Map, json};
use socketkit_core::{Page, RawRequest, Result};

use super::Api;
use crate::models::{DataSource, Database, PageOrDataSource, Paging, QueryDataSource};

/// Databases, and the data sources that hold their rows.
#[derive(Debug, Clone, Copy)]
pub struct Databases<'a>(pub(crate) Api<'a>);

impl Databases<'_> {
    /// Gets a database: its title, where it lives, and the data sources it
    /// holds. The rows and their schema belong to a data source.
    pub async fn get(&self, database: &str) -> Result<Database> {
        let path = format!("databases/{}", self.0.id("a database id", database)?);
        let body = self.0.send(RawRequest::get(path)).await?;
        self.0.object(body, "database")
    }

    /// Gets a data source: the schema its rows follow, which a filter or a
    /// sort has to name.
    pub async fn data_source(&self, data_source: &str) -> Result<DataSource> {
        let body = self.0.send(RawRequest::get(self.path(data_source)?)).await?;
        self.0.object(body, "data_source")
    }

    /// Lists the rows of a data source, each a page, that pass a filter, in
    /// the order asked for. Changes nothing; Notion offers it only as a POST.
    pub async fn query(&self, data_source: &str, query: QueryDataSource) -> Result<Page<PageOrDataSource>> {
        let mut body = Map::new();
        // A filter is sent as it was written: a `null` inside one is a value.
        body.extend(query.filter.map(|filter| ("filter".to_owned(), filter)));
        body.extend(query.sorts.map(|sorts| ("sorts".to_owned(), json!(sorts))));
        let paging = Paging {
            cursor: query.cursor,
            limit: query.limit,
        };
        let path = format!("{}/query", self.path(data_source)?);
        let request = self.0.asking(path, body, &paging)?;
        self.0.list(self.0.send(request).await?, "rows")
    }

    fn path(&self, data_source: &str) -> Result<String> {
        Ok(format!("data_sources/{}", self.0.id("a data source id", data_source)?))
    }
}
