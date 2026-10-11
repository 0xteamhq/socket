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
fn a_message_made_to_be_slow_to_read_is_read_in_one_pass() {
    let started = std::time::Instant::now();
    let open = "<".repeat(400_000);
    assert!(text_of(&format!("<p>{open} far away ></p>")).starts_with("<<<<"));
    // Many scripts, each of which is skipped to its end.
    let scripts = "<script>x</script>".repeat(100_000);
    assert_eq!(text_of(&format!("{scripts}<p>Hello</p>")), "Hello");
    // A script that never ends hides the rest, once.
    assert_eq!(text_of(&format!("<p>Hi</p><SCRIPT>{open}")), "Hi");
    assert!(
        started.elapsed() < std::time::Duration::from_secs(20),
        "{:?}",
        started.elapsed()
    );
}
