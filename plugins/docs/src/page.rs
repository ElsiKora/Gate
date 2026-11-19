//! What the layout reads of an index page: its level-2 sections and the links under one of them.

use pulldown_cmark::{Event, HeadingLevel, Parser, Tag, TagEnd};

/// A level-2 heading: its text and its line.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Heading {
    pub title: String,
    pub line: usize,
}

/// The level-2 headings of `text`, in order (none from code blocks).
pub fn sections(text: &str) -> Vec<Heading> {
    let mut headings = Vec::new();
    let mut open: Option<(String, usize)> = None;
    for (event, range) in Parser::new_ext(text, syntax()).into_offset_iter() {
        match event {
            Event::Start(Tag::Heading {
                level: HeadingLevel::H2,
                ..
            }) => open = Some((String::new(), line(text, range.start))),
            Event::Text(part) | Event::Code(part) => {
                if let Some((title, _)) = open.as_mut() {
                    title.push_str(&part);
                }
            }
            Event::End(TagEnd::Heading(HeadingLevel::H2)) => {
                if let Some((title, line)) = open.take() {
                    headings.push(Heading {
                        title: title.trim().to_owned(),
                        line,
                    });
                }
            }
            _ => {}
        }
    }
    headings
}

/// The link destinations under the level-2 section `title` of `text`, in order.
pub fn links_under(text: &str, title: &str) -> Vec<String> {
    let mut links = Vec::new();
    let mut current = String::new();
    let mut in_heading = false;
    for event in Parser::new_ext(text, syntax()) {
        match event {
            Event::Start(Tag::Heading {
                level: HeadingLevel::H1 | HeadingLevel::H2,
                ..
            }) => {
                in_heading = true;
                current.clear();
            }
            Event::Text(part) | Event::Code(part) if in_heading => current.push_str(&part),
            Event::End(TagEnd::Heading(_)) => in_heading = false,
            Event::Start(Tag::Link { dest_url, .. }) if current.trim() == title => {
                links.push(dest_url.into_string());
            }
            _ => {}
        }
    }
    links
}

/// The 1-based line of byte `offset` in `text`.
pub fn line(text: &str, offset: usize) -> usize {
    text.get(..offset)
        .map_or(0, |before| before.matches('\n').count())
        + 1
}

fn syntax() -> pulldown_cmark::Options {
    pulldown_cmark::Options::ENABLE_TABLES
        | pulldown_cmark::Options::ENABLE_STRIKETHROUGH
        | pulldown_cmark::Options::ENABLE_FOOTNOTES
}
