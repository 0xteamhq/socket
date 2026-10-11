//! Reading a Teams message as plain text: HTML with mention tags and attachment references.

use serde_json::{Value, json};
use socketkit_microsoft::models::ChatMessage;

fn message(body: Value, extra: Value) -> ChatMessage {
    let mut message = json!({ "id": "1", "body": body });
    message
        .as_object_mut()
        .unwrap()
        .extend(extra.as_object().unwrap().clone());
    serde_json::from_value(message).unwrap()
}

fn text_of(html: &str) -> String {
    message(json!({ "contentType": "html", "content": html }), json!({})).plain_text()
}

#[test]
fn a_mention_is_written_as_the_name_of_who_was_mentioned() {
    let html = "<div><at id=\"0\">Grace Hopper</at>, can you review this with <at id=\"1\">Launch crew</at>?</div>";
    assert_eq!(text_of(html), "@Grace Hopper, can you review this with @Launch crew?");
}

#[test]
fn paragraphs_and_breaks_become_lines_and_other_markup_is_dropped() {
    let html = "<p>We ship on <b>Friday</b>.</p><p>Two things:<br>docs<br/>and <i>tests</i></p>\
                <ul><li>one</li><li>two</li></ul><div>Done</div>";
    assert_eq!(
        text_of(html),
        "We ship on Friday.\nTwo things:\ndocs\nand tests\none\ntwo\nDone"
    );
}

#[test]
fn what_html_escapes_is_read_back() {
    assert_eq!(
        text_of("<p>if x &lt; 5 &amp;&amp; y &gt; 2 then &quot;go&quot; &#39;now&#39;&nbsp;&#8212; &#x2014; ok</p>"),
        "if x < 5 && y > 2 then \"go\" 'now' \u{2014} \u{2014} ok"
    );
    // An escape that is not one is kept as it was written.
    assert_eq!(text_of("AT&T &notathing; &#xZZ; &"), "AT&T &notathing; &#xZZ; &");
}

#[test]
fn an_attachment_is_named_where_the_message_refers_to_it() {
    let attached = json!({ "attachments": [
        { "id": "a1", "contentType": "reference", "name": "plan.pdf", "contentUrl": "https://contoso.sharepoint.com/plan.pdf" },
        { "id": "a2", "contentType": "application/vnd.microsoft.card.adaptive", "name": null, "content": "{}" }
    ] });
    let body = json!({ "contentType": "html", "content": "<p>Here it is</p><attachment id=\"a1\"></attachment><attachment id=\"a2\"></attachment><attachment id=\"gone\"></attachment>" });
    assert_eq!(
        message(body, attached).plain_text(),
        "Here it is\n[attachment: plan.pdf]\n[attachment]\n[attachment]"
    );
}

#[test]
fn pictures_and_emoji_are_read_by_what_they_stand_for() {
    let html = "<p>Nice <emoji id=\"smile\" alt=\"\u{1F600}\" title=\"Grinning\"></emoji> \
                <img src=\"https://x.test/a.png\" alt=\"A chart of sales\" width=\"10\"> <img src=\"y\"></p>";
    assert_eq!(text_of(html), "Nice \u{1F600} A chart of sales");
}

#[test]
fn a_link_keeps_its_words_and_its_address_when_they_differ() {
    assert_eq!(
        text_of(
            "<p>See <a href=\"https://contoso.example/plan\">the plan</a> and <a href=\"https://x.test\">https://x.test</a></p>"
        ),
        "See the plan (https://contoso.example/plan) and https://x.test"
    );
}

#[test]
fn a_sign_that_opens_no_tag_is_something_that_was_said() {
    assert_eq!(text_of("1 < 2 and 3 > 2"), "1 < 2 and 3 > 2");
    assert_eq!(text_of("a <b>bold</b> claim: x<y"), "a bold claim: x<y");
    assert_eq!(text_of("ends with <"), "ends with <");
}

#[test]
fn what_a_script_or_a_style_holds_is_not_what_was_said() {
    assert_eq!(
        text_of("<style>p { color: red }</style><p>Hello</p><script>alert('x')</script>"),
        "Hello"
    );
}

#[test]
fn a_plain_text_body_is_returned_as_it_is() {
    let body = json!({ "contentType": "text", "content": "x < 5 & <b>not bold</b>\n  kept  as written" });
    assert_eq!(
        message(body, json!({})).plain_text(),
        "x < 5 & <b>not bold</b>\n  kept  as written"
    );
}

#[test]
fn a_message_with_no_body_or_a_deleted_one_reads_as_nothing() {
    assert_eq!(message(json!(null), json!({})).plain_text(), "");
    let deleted = json!({ "deletedDateTime": "2026-10-09T08:15:00Z" });
    assert_eq!(
        message(json!({ "contentType": "html", "content": "" }), deleted).plain_text(),
        ""
    );
    assert_eq!(text_of("<p>&nbsp;</p><div><br></div>"), "");
}

#[test]
fn cells_of_a_table_are_kept_apart() {
    // Two numbers in neighbouring cells are two numbers.
    let html = "<table><tr><td>Budget</td><td>15</td><td>000</td></tr><tr><th>Q1</th><th>Q2</th></tr></table>";
    assert_eq!(text_of(html), "Budget | 15 | 000\nQ1 | Q2");
}

#[test]
fn what_a_tag_holds_in_its_attributes_is_not_what_was_said() {
    // A `>` inside a quoted value does not end the tag, so a tooltip or an
    // address a reader never sees is not read out as part of the message.
    assert_eq!(
        text_of("<a href=\"https://x.test/\" title=\"> Approved by your manager\">click</a>"),
        "click (https://x.test/)"
    );
    assert_eq!(
        text_of("<p>Chart: <img alt=\"a > b\" src=\"https://x.test/secret-token\"></p>"),
        "Chart: a > b"
    );
    // An attribute is found by its own name, not inside another's value.
    assert_eq!(text_of("<img title=\"x alt='SPOOFED'\" alt=\"real\">"), "real");
    assert_eq!(text_of("<img data-alt=\"no\" ALT = 'yes'>"), "yes");
    assert_eq!(text_of("<a data-href=\"https://no.test\">words</a>"), "words");
    assert_eq!(text_of("<img alt=unquoted src=x>"), "unquoted");
    // A comment is not shown, whatever it holds.
    assert_eq!(text_of("<!-- hidden > note --><p>seen</p>"), "seen");
    assert_eq!(text_of("<!-- <b>hidden</b> --><p>seen</p>"), "seen");
    assert_eq!(text_of("<p>seen</p><!-- never closed <p>not seen</p>"), "seen");
    // A tag that never ends is not one: it is shown as it was written.
    assert_eq!(text_of("a <b title=\"never closed"), "a <b title=\"never closed");
}

#[test]
fn what_graph_sends_as_text_is_not_taken_for_the_rendering() {
    // `text` is Socket's own reading of the body. A message that arrives with
    // one, or with `null` there, is read all the same and by its body alone.
    for sent in [json!("made up"), json!(null)] {
        let read = message(
            json!({ "contentType": "html", "content": "<p>real</p>" }),
            json!({ "text": sent }),
        );
        assert_eq!(read.plain_text(), "real");
    }
}

#[test]
fn a_message_made_to_be_slow_to_read_is_read_in_one_pass() {
    // Each of these is read once from end to end. Reading the rest of the
    // message again for every `<` or `&` in it would take minutes at this
    // size, and a message may be ten megabytes.
    let started = std::time::Instant::now();
    let many = 1_500_000;
    let open = "<".repeat(many);
    assert!(text_of(&format!("<p>{open} far away ></p>")).starts_with("<<<<"));
    assert_eq!(text_of(&"&".repeat(many)).len(), many);
    assert_eq!(text_of(&"a&b ".repeat(many / 4)).len(), many - 1);
    assert_eq!(text_of(&format!("<img alt=\"{}\">", "&".repeat(many))).len(), many);
    // Tags that open a quote and never close it.
    assert!(text_of(&"<a \"".repeat(many / 4)).starts_with("<a \""));
    assert!(text_of(&"<a x=\"<\" ".repeat(many / 10)).starts_with("<a x="));
    // Many scripts, each of which is skipped to its end.
    let scripts = "<script>x</script>".repeat(100_000);
    assert_eq!(text_of(&format!("{scripts}<p>Hello</p>")), "Hello");
    // A script that never ends hides the rest, once.
    assert_eq!(text_of(&format!("<p>Hi</p><SCRIPT>{open}")), "Hi");
    assert!(
        started.elapsed() < std::time::Duration::from_secs(30),
        "{:?}",
        started.elapsed()
    );
}
