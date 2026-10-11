//! A Google document exported as text: the formats on offer, and what comes back.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// The text formats a Google document can be exported in. Each is written in
/// JSON as the MIME type Google knows it by.
///
/// The choice is closed on purpose. Google also exports to PDF, Word, Excel
/// and other formats that are bytes and not text. An export returns text, so
/// those are not offered.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub enum DriveExportFormat {
    /// A Doc or a Slides presentation as plain text.
    #[serde(rename = "text/plain")]
    Text,
    /// A Doc as Markdown, which keeps its headings, lists, links and tables.
    #[serde(rename = "text/markdown")]
    Markdown,
    /// A Sheet as comma-separated values. Only its first sheet is exported.
    #[serde(rename = "text/csv")]
    Csv,
}

impl DriveExportFormat {
    /// The MIME type Google is asked for.
    pub fn mime_type(self) -> &'static str {
        match self {
            Self::Text => "text/plain",
            Self::Markdown => "text/markdown",
            Self::Csv => "text/csv",
        }
    }
}

/// A Google document as text.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct DriveExport {
    /// The format the text is in: the one that was asked for.
    pub mime_type: DriveExportFormat,
    /// The whole document. Empty when the document is empty.
    pub text: String,
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn a_format_is_written_in_json_as_the_mime_type_google_is_asked_for() {
        for format in [
            DriveExportFormat::Text,
            DriveExportFormat::Markdown,
            DriveExportFormat::Csv,
        ] {
            assert_eq!(serde_json::to_value(format).unwrap(), json!(format.mime_type()));
            let read: DriveExportFormat = serde_json::from_value(json!(format.mime_type())).unwrap();
            assert_eq!(read, format);
        }
    }

    #[test]
    fn a_format_that_is_not_text_is_not_a_format() {
        for other in [
            "application/pdf",
            "text/html",
            "TEXT/PLAIN",
            "text/plain; charset=utf-8",
            "markdown",
            "",
        ] {
            assert!(
                serde_json::from_value::<DriveExportFormat>(json!(other)).is_err(),
                "{other:?}"
            );
        }
    }
}
