// This code is inlined from
// https://github.com/arranf/strip-markdown
// because the original version breaks clippy runs.
// See https://github.com/arranf/strip-markdown/issues/2
// MIT License

use pulldown_cmark::Event::{End, HardBreak, SoftBreak, Start, Text};
use pulldown_cmark::{Parser, Tag};

pub fn strip_markdown(markdown: &str) -> String {
    let parser = Parser::new(&markdown);
    let mut buffer = String::new();
    for event in parser {
        debug!("{:?}", event);
        match event {
            Start(tag) => start_tag(tag, &mut buffer),
            End(tag) => end_tag(tag, &mut buffer),
            Text(text) => {
                debug!("Pushing {}", &text);
                buffer.push_str(&text);
            }
            SoftBreak => fresh_line(&mut buffer),
            HardBreak => fresh_line(&mut buffer),
            _ => (),
        }
    }
    buffer
}

fn start_tag(tag: Tag, buffer: &mut String) {
    match tag {
        Tag::Paragraph => (),
        // Tag::Rule => fresh_line(buffer),
        Tag::Heading(_level) => (),
        Tag::Table(_alignments) => (),
        Tag::TableHead => (),
        Tag::TableRow => (),
        Tag::TableCell => (),
        Tag::BlockQuote => (),
        Tag::CodeBlock(_info) => fresh_hard_break(buffer),
        Tag::List(_number) => fresh_line(buffer),
        Tag::Item => (),
        Tag::Emphasis => (),
        Tag::Strong => (),
        Tag::Strikethrough => (),
        Tag::Link(_type, _dest, title) => {
            if !title.is_empty() {
                buffer.push_str(&title);
            }
        }
        Tag::Image(_type, _dest, title) => {
            if !title.is_empty() {
                buffer.push_str(&title);
            }
        }
        Tag::FootnoteDefinition(_) => (),
    }
}

fn end_tag(tag: Tag, buffer: &mut String) {
    match tag {
        Tag::Paragraph => (),
        // Tag::Rule => (),
        Tag::Heading(_) => fresh_line(buffer),
        Tag::Table(_) => {
            fresh_line(buffer);
        }
        Tag::TableHead => {
            fresh_line(buffer);
        }
        Tag::TableRow => {
            fresh_line(buffer);
        }
        Tag::BlockQuote => fresh_line(buffer),
        Tag::CodeBlock(_) => fresh_line(buffer),
        Tag::List(_) => (),
        Tag::Item => fresh_line(buffer),
        Tag::Image(_, _, _) => (), // shouldn't happen, handled in start
        Tag::FootnoteDefinition(_) => (),
        _ => (),
    }
}

fn fresh_line(buffer: &mut String) {
    debug!("Pushing \\n");
    buffer.push('\n');
}

fn fresh_hard_break(buffer: &mut String) {
    debug!("Pushing \\n\\n");
    buffer.push_str("\n\n");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn basic_inline_strong() {
        let markdown = r#"**Hello**"#;
        let expected = "Hello";
        assert_eq!(strip_markdown(markdown), expected);
    }

    #[test]
    fn basic_inline_emphasis() {
        let markdown = r#"_Hello_"#;
        let expected = "Hello";
        assert_eq!(strip_markdown(markdown), expected);
    }

    #[test]
    fn basic_header() {
        let markdown = r#"# Header"#;
        let expected = "Header\n";
        assert_eq!(strip_markdown(markdown), expected);
    }

    #[test]
    fn alt_header() {
        let markdown = r#"
Header
======
"#;
        let expected = "Header\n";
        assert_eq!(strip_markdown(markdown), expected);
    }

    #[test]
    fn strong_emphasis() {
        let markdown = r#"**asterisks and _underscores_**"#;
        let expected = "asterisks and underscores";
        assert_eq!(strip_markdown(markdown), expected);
    }

    #[ignore]
    #[test]
    fn strikethrough() {
        let markdown = r#"~~strikethrough~~"#;
        let expected = "strikethrough";
        assert_eq!(strip_markdown(markdown), expected);
    }

    #[test]
    fn mixed_list() {
        let markdown = r#"
1. First ordered list item
2. Another item 
1. Actual numbers don't matter, just that it's a number
  1. Ordered sub-list
4. And another item.
"#;

        let expected = r#"
First ordered list item
Another item
Actual numbers don't matter, just that it's a number
Ordered sub-list
And another item.
"#;
        assert_eq!(strip_markdown(markdown), expected);
    }

    #[test]
    fn basic_list() {
        let markdown = r#"
* alpha
* beta
"#;
        let expected = r#"
alpha
beta
"#;
        assert_eq!(strip_markdown(markdown), expected);
    }

    #[test]
    fn list_with_header() {
        let markdown = r#"# Title
* alpha
* beta
"#;
        let expected = r#"Title
alpha
beta
"#;
        assert_eq!(strip_markdown(markdown), expected);
    }

    #[test]
    fn basic_link() {
        let markdown = "[I'm an inline-style link](https://www.google.com)";
        let expected = "I'm an inline-style link";
        assert_eq!(strip_markdown(markdown), expected)
    }

    #[ignore]
    #[test]
    fn link_with_itself() {
        let markdown = "[https://www.google.com]";
        let expected = "https://www.google.com";
        assert_eq!(strip_markdown(markdown), expected)
    }

    #[test]
    fn basic_image() {
        let markdown = "![alt text](https://github.com/adam-p/markdown-here/raw/master/src/common/images/icon48.png)";
        let expected = "alt text";
        assert_eq!(strip_markdown(markdown), expected);
    }

    #[test]
    fn inline_code() {
        let markdown = "`inline code`";
        let expected = "inline code";
        assert_eq!(strip_markdown(markdown), expected);
    }

    #[test]
    fn code_block() {
        let markdown = r#"
```javascript
var s = "JavaScript syntax highlighting";
alert(s);
```"#;
        let expected = r#"
var s = "JavaScript syntax highlighting";
alert(s);
"#;
        assert_eq!(strip_markdown(markdown), expected);
    }

    #[test]
    fn block_quote() {
        let markdown = r#"> Blockquotes are very handy in email to emulate reply text.
> This line is part of the same quote."#;
        let expected = "Blockquotes are very handy in email to emulate reply text.
This line is part of the same quote.\n";
        assert_eq!(strip_markdown(markdown), expected);
    }
}
