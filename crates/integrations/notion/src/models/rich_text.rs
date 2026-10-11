//! Text with its formatting, as Notion writes it everywhere text appears.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// One run of text with one formatting. A paragraph, a title or a comment is
/// a list of these.
///
/// To write plain text, [`RichText::plain`] is enough.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct RichText {
    /// `text`, `mention` or `equation`.
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<Text>,
    /// A person, a page, a database or a date named in the text, as Notion writes it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mention: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub equation: Option<Equation>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub annotations: Option<Annotations>,
    /// The run without its formatting. Notion writes it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub plain_text: Option<String>,
    /// Where the run links to, a mention's page included. Notion writes it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub href: Option<String>,
}

/// The content of a run of ordinary text.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct Text {
    /// At most 2000 characters in one run.
    pub content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub link: Option<Link>,
}

/// A link on a run of text.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct Link {
    pub url: String,
}

/// A formula written inline.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct Equation {
    /// The formula, in LaTeX.
    pub expression: String,
}

/// How a run of text is formatted.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct Annotations {
    pub bold: bool,
    pub italic: bool,
    pub strikethrough: bool,
    pub underline: bool,
    pub code: bool,
    /// `default`, a colour such as `blue`, or a background such as `blue_background`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
}

impl RichText {
    /// A run of text with no formatting.
    pub fn plain(content: impl Into<String>) -> Self {
        Self {
            kind: Some("text".to_owned()),
            text: Some(Text {
                content: content.into(),
                link: None,
            }),
            ..Self::default()
        }
    }

    /// What the run says, without its formatting.
    fn said(&self) -> &str {
        let written = self.text.as_ref().map(|text| text.content.as_str());
        let formula = self.equation.as_ref().map(|equation| equation.expression.as_str());
        self.plain_text.as_deref().or(written).or(formula).unwrap_or_default()
    }

    /// The run as Markdown: its formatting as marks around it, and its link.
    ///
    /// Marks are put around the words and not around the spaces beside them,
    /// since `** bold**` is not bold. Underline has no mark and is dropped.
    fn markdown(&self) -> String {
        let said = match &self.equation {
            Some(equation) => format!("${}$", equation.expression),
            None => self.said().to_owned(),
        };
        let words = said.trim();
        if words.is_empty() {
            return said;
        }
        let before = &said[..said.len() - said.trim_start().len()];
        let after = &said[said.trim_end().len()..];
        let mut marked = words.to_owned();
        if let Some(annotations) = &self.annotations {
            let marks = [
                (annotations.code, "`"),
                (annotations.bold, "**"),
                (annotations.italic, "*"),
                (annotations.strikethrough, "~~"),
            ];
            for (_, mark) in marks.into_iter().filter(|(set, _)| *set) {
                marked = format!("{mark}{marked}{mark}");
            }
        }
        let link = self.text.as_ref().and_then(|text| text.link.as_ref());
        let address = self.href.as_deref().or(link.map(|link| link.url.as_str()));
        if let Some(address) = address.filter(|address| !address.is_empty()) {
            marked = format!("[{marked}]({address})");
        }
        format!("{before}{marked}{after}")
    }
}

/// What a list of runs says, without formatting.
pub(crate) fn plain_text(runs: &[RichText]) -> String {
    runs.iter().map(RichText::said).collect()
}

/// A list of runs as one piece of Markdown.
pub(super) fn markdown(runs: &[RichText]) -> String {
    runs.iter().map(RichText::markdown).collect()
}

/// Reads the runs Notion wrote at `value`. Anything that is not a list of
/// runs reads as no text.
pub(super) fn runs(value: &Value) -> Vec<RichText> {
    serde_json::from_value(value.clone()).unwrap_or_default()
}
