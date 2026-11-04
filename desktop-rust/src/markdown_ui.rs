// SPDX-License-Identifier: Apache-2.0
//
// 助手正文 Markdown → 结构化 UI。
// 用 pulldown-cmark 真解析，禁假 strip-MD（去掉标记却无样式 / 表仍源码）。

use eframe::egui::{self, Color32, RichText, Rounding, Stroke};
use pulldown_cmark::{CodeBlockKind, Event, HeadingLevel, Options, Parser, Tag, TagEnd};

use crate::theme;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Inline {
    Text(String),
    Strong(Vec<Inline>),
    Emphasis(Vec<Inline>),
    Strikethrough(Vec<Inline>),
    Code(String),
    Link { text: Vec<Inline>, dest: String },
    SoftBreak,
    HardBreak,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Block {
    Heading {
        level: u8,
        spans: Vec<Inline>,
    },
    Paragraph {
        spans: Vec<Inline>,
    },
    List {
        ordered: bool,
        start: u64,
        items: Vec<Vec<Block>>,
    },
    CodeBlock {
        lang: String,
        code: String,
    },
    Table {
        headers: Vec<Vec<Inline>>,
        rows: Vec<Vec<Vec<Inline>>>,
    },
    Quote {
        children: Vec<Block>,
    },
    Rule,
}

#[derive(Debug, Clone, Copy)]
pub struct MarkdownView {
    pub streaming: bool,
    pub show_role: bool,
}

impl Default for MarkdownView {
    fn default() -> Self {
        Self {
            streaming: false,
            show_role: true,
        }
    }
}

pub fn parse_markdown(src: &str) -> Vec<Block> {
    let mut opts = Options::empty();
    opts.insert(Options::ENABLE_TABLES);
    opts.insert(Options::ENABLE_STRIKETHROUGH);
    let events: Vec<Event<'_>> = Parser::new_ext(src, opts).collect();
    let mut i = 0;
    parse_blocks(&events, &mut i, None)
}

fn is_end(event: &Event<'_>, kind: TagEnd) -> bool {
    matches!(event, Event::End(end) if *end == kind)
}

pub fn show_markdown(ui: &mut egui::Ui, src: &str, view: MarkdownView) {
    let mut blocks = parse_markdown(src);
    if view.streaming {
        append_cursor(&mut blocks);
    }
    // 父 Ui 若是 left_to_right，Frame 会原样继承，块就会挤在同一行，
    // 超出视口宽度的代码围栏和表格被纵向滚动裁掉。这里锁成纵向列。
    ui.vertical(|ui| {
        let width = ui.available_width();
        if width.is_finite() && width > 0.0 {
            ui.set_width(width);
        }
        if view.show_role {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 6.0;
                ui.label(RichText::new("✦").size(13.0).color(theme::accent_bright()));
                ui.label(
                    RichText::new("助手")
                        .size(12.0)
                        .strong()
                        .color(theme::accent_soft()),
                );
            });
            ui.add_space(8.0);
        }
        let spacing = ui.spacing_mut();
        spacing.item_spacing.y = 8.0;
        spacing.item_spacing.x = 8.0;
        render_blocks(ui, &blocks, 0);
    });
}

fn parse_blocks(events: &[Event<'_>], i: &mut usize, until: Option<TagEnd>) -> Vec<Block> {
    let mut out = Vec::new();
    while *i < events.len() {
        if let Some(kind) = until {
            if is_end(&events[*i], kind) {
                *i += 1;
                break;
            }
        }
        match &events[*i] {
            Event::Start(tag) => match tag {
                Tag::Paragraph => {
                    *i += 1;
                    let spans = parse_inlines(events, i, TagEnd::Paragraph);
                    out.push(Block::Paragraph { spans });
                }
                Tag::Heading { level, .. } => {
                    let heading = *level;
                    *i += 1;
                    let spans = parse_inlines(events, i, TagEnd::Heading(heading));
                    out.push(Block::Heading {
                        level: heading_level(heading),
                        spans,
                    });
                }
                Tag::List(start) => {
                    let ordered = start.is_some();
                    let start = start.unwrap_or(1);
                    *i += 1;
                    let items = parse_list_items(events, i);
                    out.push(Block::List {
                        ordered,
                        start,
                        items,
                    });
                }
                Tag::CodeBlock(kind) => {
                    let lang = match kind {
                        CodeBlockKind::Fenced(lang) => lang.to_string(),
                        CodeBlockKind::Indented => String::new(),
                    };
                    *i += 1;
                    let code = collect_code(events, i);
                    out.push(Block::CodeBlock { lang, code });
                }
                Tag::Table(_) => {
                    *i += 1;
                    out.push(parse_table(events, i));
                }
                Tag::BlockQuote(kind) => {
                    let end = TagEnd::BlockQuote(*kind);
                    *i += 1;
                    let children = parse_blocks(events, i, Some(end));
                    out.push(Block::Quote { children });
                }
                Tag::Item => {
                    *i += 1;
                    let item = parse_item_blocks(events, i);
                    out.push(Block::List {
                        ordered: false,
                        start: 1,
                        items: vec![item],
                    });
                }
                other => {
                    let end = other.to_end();
                    *i += 1;
                    let _ = parse_inlines(events, i, end);
                }
            },
            Event::Rule => {
                out.push(Block::Rule);
                *i += 1;
            }
            Event::End(_) => break,
            Event::Text(text) => {
                let mut spans = vec![Inline::Text(text.to_string())];
                *i += 1;
                spans.extend(parse_inlines(events, i, TagEnd::Paragraph));
                out.push(Block::Paragraph { spans });
            }
            _ => *i += 1,
        }
    }
    out
}

fn parse_list_items(events: &[Event<'_>], i: &mut usize) -> Vec<Vec<Block>> {
    let mut items = Vec::new();
    while *i < events.len() {
        match &events[*i] {
            Event::Start(Tag::Item) => {
                *i += 1;
                items.push(parse_item_blocks(events, i));
            }
            Event::End(TagEnd::List(_)) => {
                *i += 1;
                break;
            }
            Event::End(_) => break,
            _ => *i += 1,
        }
    }
    items
}

fn parse_item_blocks(events: &[Event<'_>], i: &mut usize) -> Vec<Block> {
    let mut blocks = Vec::new();
    let mut loose = Vec::new();
    while *i < events.len() {
        match &events[*i] {
            Event::End(TagEnd::Item) => {
                *i += 1;
                break;
            }
            Event::Start(Tag::Paragraph)
            | Event::Start(Tag::List(_))
            | Event::Start(Tag::Heading { .. })
            | Event::Start(Tag::CodeBlock(_))
            | Event::Start(Tag::Table(_))
            | Event::Start(Tag::BlockQuote(_)) => {
                flush_loose(&mut loose, &mut blocks);
                let nested = parse_blocks(events, i, None);
                blocks.extend(nested);
            }
            Event::End(TagEnd::List(_)) => break,
            Event::SoftBreak | Event::HardBreak => {
                loose.push(if matches!(events[*i], Event::HardBreak) {
                    Inline::HardBreak
                } else {
                    Inline::SoftBreak
                });
                *i += 1;
            }
            Event::Text(text) => {
                loose.push(Inline::Text(text.to_string()));
                *i += 1;
            }
            Event::Code(code) => {
                loose.push(Inline::Code(code.to_string()));
                *i += 1;
            }
            Event::Start(tag) => {
                let dest = match tag {
                    Tag::Link { dest_url, .. } => Some(dest_url.to_string()),
                    _ => None,
                };
                let end = tag.to_end();
                *i += 1;
                let inner = parse_inlines(events, i, end);
                match tag {
                    Tag::Strong => loose.push(Inline::Strong(inner)),
                    Tag::Emphasis => loose.push(Inline::Emphasis(inner)),
                    Tag::Strikethrough => loose.push(Inline::Strikethrough(inner)),
                    Tag::Link { .. } => loose.push(Inline::Link {
                        text: inner,
                        dest: dest.unwrap_or_default(),
                    }),
                    _ => loose.extend(inner),
                }
            }
            _ => *i += 1,
        }
    }
    flush_loose(&mut loose, &mut blocks);
    if blocks.is_empty() {
        blocks.push(Block::Paragraph { spans: Vec::new() });
    }
    blocks
}

fn flush_loose(loose: &mut Vec<Inline>, blocks: &mut Vec<Block>) {
    if loose
        .iter()
        .any(|span| !matches!(span, Inline::SoftBreak | Inline::HardBreak))
    {
        blocks.push(Block::Paragraph {
            spans: std::mem::take(loose),
        });
    } else {
        loose.clear();
    }
}

fn parse_table(events: &[Event<'_>], i: &mut usize) -> Block {
    let mut headers = Vec::new();
    let mut rows = Vec::new();
    while *i < events.len() {
        match &events[*i] {
            Event::Start(Tag::TableHead) => {
                *i += 1;
                headers = parse_table_cells(events, i, TagEnd::TableHead);
            }
            Event::Start(Tag::TableRow) => {
                *i += 1;
                rows.push(parse_table_cells(events, i, TagEnd::TableRow));
            }
            Event::End(TagEnd::Table) => {
                *i += 1;
                break;
            }
            Event::End(_) => {
                *i += 1;
                break;
            }
            _ => *i += 1,
        }
    }
    Block::Table { headers, rows }
}

fn parse_table_cells(events: &[Event<'_>], i: &mut usize, until: TagEnd) -> Vec<Vec<Inline>> {
    let mut cells = Vec::new();
    while *i < events.len() {
        if is_end(&events[*i], until) {
            *i += 1;
            break;
        }
        match &events[*i] {
            Event::Start(Tag::TableCell) => {
                *i += 1;
                cells.push(parse_inlines(events, i, TagEnd::TableCell));
            }
            Event::End(_) => {
                *i += 1;
                break;
            }
            _ => *i += 1,
        }
    }
    cells
}

fn parse_inlines(events: &[Event<'_>], i: &mut usize, until: TagEnd) -> Vec<Inline> {
    let mut spans = Vec::new();
    while *i < events.len() {
        if is_end(&events[*i], until) {
            *i += 1;
            break;
        }
        match &events[*i] {
            Event::Text(text) => {
                spans.push(Inline::Text(text.to_string()));
                *i += 1;
            }
            Event::Code(code) => {
                spans.push(Inline::Code(code.to_string()));
                *i += 1;
            }
            Event::SoftBreak => {
                spans.push(Inline::SoftBreak);
                *i += 1;
            }
            Event::HardBreak => {
                spans.push(Inline::HardBreak);
                *i += 1;
            }
            Event::Start(Tag::Strong) => {
                *i += 1;
                spans.push(Inline::Strong(parse_inlines(events, i, TagEnd::Strong)));
            }
            Event::Start(Tag::Emphasis) => {
                *i += 1;
                spans.push(Inline::Emphasis(parse_inlines(events, i, TagEnd::Emphasis)));
            }
            Event::Start(Tag::Strikethrough) => {
                *i += 1;
                spans.push(Inline::Strikethrough(parse_inlines(
                    events,
                    i,
                    TagEnd::Strikethrough,
                )));
            }
            Event::Start(Tag::Link { dest_url, .. }) => {
                let dest = dest_url.to_string();
                *i += 1;
                spans.push(Inline::Link {
                    text: parse_inlines(events, i, TagEnd::Link),
                    dest,
                });
            }
            Event::Start(Tag::Image {
                dest_url, title, ..
            }) => {
                let label = if title.is_empty() {
                    dest_url.to_string()
                } else {
                    title.to_string()
                };
                *i += 1;
                let _ = parse_inlines(events, i, TagEnd::Image);
                spans.push(Inline::Text(label));
            }
            Event::Html(html) | Event::InlineHtml(html) => {
                spans.push(Inline::Text(html.to_string()));
                *i += 1;
            }
            Event::End(_) => break,
            Event::Start(_) => *i += 1,
            _ => *i += 1,
        }
    }
    spans
}

fn collect_code(events: &[Event<'_>], i: &mut usize) -> String {
    let mut code = String::new();
    while *i < events.len() {
        match &events[*i] {
            Event::Text(text) => {
                code.push_str(text);
                *i += 1;
            }
            Event::End(TagEnd::CodeBlock) => {
                *i += 1;
                break;
            }
            Event::End(_) => {
                *i += 1;
                break;
            }
            _ => *i += 1,
        }
    }
    if code.ends_with('\n') {
        code.pop();
    }
    code
}

fn heading_level(level: HeadingLevel) -> u8 {
    match level {
        HeadingLevel::H1 => 1,
        HeadingLevel::H2 => 2,
        HeadingLevel::H3 => 3,
        HeadingLevel::H4 => 4,
        HeadingLevel::H5 => 5,
        HeadingLevel::H6 => 6,
    }
}

fn append_cursor(blocks: &mut [Block]) {
    if let Some(last) = blocks.last_mut() {
        append_cursor_block(last);
    }
}

fn append_cursor_block(block: &mut Block) {
    match block {
        Block::Paragraph { spans } | Block::Heading { spans, .. } => {
            spans.push(Inline::Text("▌".into()))
        }
        Block::List { items, .. } => {
            if let Some(item) = items.last_mut() {
                if let Some(child) = item.last_mut() {
                    append_cursor_block(child);
                }
            }
        }
        Block::Quote { children } => {
            if let Some(child) = children.last_mut() {
                append_cursor_block(child);
            }
        }
        Block::CodeBlock { code, .. } => code.push('▌'),
        Block::Table { rows, .. } => {
            if let Some(row) = rows.last_mut() {
                if let Some(cell) = row.last_mut() {
                    cell.push(Inline::Text("▌".into()));
                }
            }
        }
        Block::Rule => {}
    }
}

fn render_blocks(ui: &mut egui::Ui, blocks: &[Block], depth: usize) {
    for (idx, block) in blocks.iter().enumerate() {
        if idx > 0 {
            ui.add_space(2.0);
        }
        match block {
            Block::Heading { level, spans } => {
                let size = match level {
                    1 => 22.0,
                    2 => 18.0,
                    3 => 16.0,
                    _ => 14.5,
                };
                render_spans(ui, spans, size, true, theme::text_primary());
            }
            Block::Paragraph { spans } => {
                render_spans(ui, spans, 14.0, false, theme::text_primary());
            }
            Block::List {
                ordered,
                start,
                items,
            } => render_list(ui, *ordered, *start, items, depth),
            Block::CodeBlock { lang, code } => render_code_block(ui, lang, code),
            Block::Table { headers, rows } => render_table(ui, headers, rows, depth, idx),
            Block::Quote { children } => {
                egui::Frame::none()
                    .inner_margin(egui::Margin::symmetric(10.0, 6.0))
                    .rounding(Rounding::same(8.0))
                    .fill(theme::sidebar_bg())
                    .stroke(Stroke::new(1.0_f32, theme::glass_stroke()))
                    .show(ui, |ui| {
                        ui.set_min_width(ui.available_width());
                        render_blocks(ui, children, depth + 1);
                    });
            }
            Block::Rule => {
                let width = ui.available_width();
                let (rect, _) =
                    ui.allocate_exact_size(egui::vec2(width, 8.0), egui::Sense::hover());
                ui.painter().hline(
                    rect.x_range(),
                    rect.center().y,
                    Stroke::new(1.0_f32, theme::hairline()),
                );
            }
        }
    }
}

fn render_list(ui: &mut egui::Ui, ordered: bool, start: u64, items: &[Vec<Block>], depth: usize) {
    for (n, item) in items.iter().enumerate() {
        ui.horizontal(|ui| {
            let mark = if ordered {
                format!("{}.", start + n as u64)
            } else {
                "•".to_owned()
            };
            ui.add_sized(
                [18.0, 18.0],
                egui::Label::new(RichText::new(mark).size(13.0).color(theme::text_muted())),
            );
            ui.vertical(|ui| {
                ui.set_min_width((ui.available_width() - 4.0).max(40.0));
                render_blocks(ui, item, depth + 1);
            });
        });
    }
}

fn render_code_block(ui: &mut egui::Ui, lang: &str, code: &str) {
    egui::Frame::none()
        .inner_margin(egui::Margin::symmetric(12.0, 10.0))
        .rounding(Rounding::same(8.0))
        .fill(theme::ink())
        .stroke(Stroke::new(1.0_f32, theme::glass_stroke()))
        .show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            if !lang.is_empty() {
                ui.label(
                    RichText::new(lang)
                        .size(11.0)
                        .color(theme::text_dim())
                        .monospace(),
                );
                ui.add_space(4.0);
            }
            ui.label(
                RichText::new(code)
                    .monospace()
                    .size(13.0)
                    .color(theme::text_primary()),
            );
        });
}

fn render_table(
    ui: &mut egui::Ui,
    headers: &[Vec<Inline>],
    rows: &[Vec<Vec<Inline>>],
    depth: usize,
    idx: usize,
) {
    let cols = headers
        .len()
        .max(rows.iter().map(|row| row.len()).max().unwrap_or(0))
        .max(1);
    egui::Frame::none()
        .inner_margin(egui::Margin::symmetric(2.0, 2.0))
        .rounding(Rounding::same(8.0))
        .fill(theme::card_bg())
        .stroke(Stroke::new(1.0_f32, theme::glass_stroke()))
        .show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            egui::Grid::new(ui.id().with(("md_table", depth, idx)))
                .num_columns(cols)
                .spacing([10.0, 6.0])
                .min_col_width(48.0)
                .striped(true)
                .show(ui, |ui| {
                    if !headers.is_empty() {
                        for cell in headers.iter() {
                            table_cell(ui, cell, true);
                        }
                        for _ in headers.len()..cols {
                            ui.label("");
                        }
                        ui.end_row();
                    }
                    for row in rows {
                        for cell in row.iter() {
                            table_cell(ui, cell, false);
                        }
                        for _ in row.len()..cols {
                            ui.label("");
                        }
                        ui.end_row();
                    }
                });
        });
}

fn table_cell(ui: &mut egui::Ui, spans: &[Inline], header: bool) {
    egui::Frame::none()
        .inner_margin(egui::Margin::symmetric(8.0, 4.0))
        .fill(if header {
            theme::sidebar_bg()
        } else {
            Color32::TRANSPARENT
        })
        .show(ui, |ui| {
            render_spans(
                ui,
                spans,
                if header { 12.5 } else { 13.0 },
                header,
                theme::text_primary(),
            );
        });
}

fn render_spans(ui: &mut egui::Ui, spans: &[Inline], size: f32, strong: bool, color: Color32) {
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = egui::vec2(0.0, 4.0);
        ui.style_mut().override_text_style = None;
        paint_inlines(ui, spans, size, strong, color);
    });
}

fn paint_inlines(ui: &mut egui::Ui, spans: &[Inline], size: f32, strong: bool, color: Color32) {
    for span in spans {
        match span {
            Inline::Text(text) => {
                if text.is_empty() {
                    continue;
                }
                let mut rich = RichText::new(text).size(size).color(color);
                if strong {
                    rich = rich.strong();
                }
                ui.label(rich);
            }
            Inline::Strong(inner) => paint_inlines(ui, inner, size, true, color),
            Inline::Emphasis(inner) => {
                for child in inner {
                    match child {
                        Inline::Text(text) => {
                            let mut rich = RichText::new(text).size(size).color(color).italics();
                            if strong {
                                rich = rich.strong();
                            }
                            ui.label(rich);
                        }
                        other => {
                            paint_inlines(ui, std::slice::from_ref(other), size, strong, color)
                        }
                    }
                }
            }
            Inline::Strikethrough(inner) => {
                paint_inlines(ui, inner, size, strong, theme::text_muted());
            }
            Inline::Code(code) => {
                ui.label(
                    RichText::new(code)
                        .monospace()
                        .size((size - 1.0).max(12.0))
                        .color(theme::accent_soft())
                        .background_color(theme::card_bg()),
                );
            }
            Inline::Link { text, dest } => {
                let label = flatten_inlines(text);
                let shown = if label.is_empty() {
                    dest.clone()
                } else {
                    label
                };
                ui.hyperlink_to(
                    RichText::new(shown).size(size).color(theme::accent_soft()),
                    dest,
                );
            }
            Inline::SoftBreak => {
                ui.label(RichText::new(" ").size(size));
            }
            Inline::HardBreak => {
                ui.label(RichText::new("\n").size(size));
            }
        }
    }
}

pub fn flatten_inlines(spans: &[Inline]) -> String {
    let mut out = String::new();
    write_inlines(spans, &mut out);
    out
}

pub fn flatten_blocks(blocks: &[Block]) -> String {
    let mut out = String::new();
    write_blocks(blocks, &mut out);
    out
}

fn write_blocks(blocks: &[Block], out: &mut String) {
    for block in blocks {
        match block {
            Block::Heading { spans, .. } | Block::Paragraph { spans } => {
                write_inlines(spans, out);
                out.push('\n');
            }
            Block::List { items, .. } => {
                for item in items {
                    write_blocks(item, out);
                }
            }
            Block::CodeBlock { code, .. } => {
                out.push_str(code);
                out.push('\n');
            }
            Block::Table { headers, rows } => {
                for cell in headers {
                    write_inlines(cell, out);
                    out.push(' ');
                }
                out.push('\n');
                for row in rows {
                    for cell in row {
                        write_inlines(cell, out);
                        out.push(' ');
                    }
                    out.push('\n');
                }
            }
            Block::Quote { children } => write_blocks(children, out),
            Block::Rule => out.push('\n'),
        }
    }
}

fn write_inlines(spans: &[Inline], out: &mut String) {
    for span in spans {
        match span {
            Inline::Text(text) => out.push_str(text),
            Inline::Strong(inner) | Inline::Emphasis(inner) | Inline::Strikethrough(inner) => {
                write_inlines(inner, out);
            }
            Inline::Code(code) => out.push_str(code),
            Inline::Link { text, dest } => {
                if text.is_empty() {
                    out.push_str(dest);
                } else {
                    write_inlines(text, out);
                }
            }
            Inline::SoftBreak => out.push(' '),
            Inline::HardBreak => out.push('\n'),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIXTURE: &str = r#"## 文档结构概览

| 章节 | 内容要点 |
| --- | --- |
| **ck 数据库** | 法兰克林集群 |
| Kafka | `aids_bigdata` |

## 几点提醒

1. **敏感信息**：该文档含大量明文 AK/SK
2. 两个未提到的正文档里有 `sheet` 区域

```json
{ "ok": true }
```

需要我继续把 `sheet` 的内容也拉出来吗？
"#;

    #[test]
    fn settled_parse_has_headings_emphasis_code_list_table() {
        let blocks = parse_markdown(FIXTURE);
        assert!(
            blocks
                .iter()
                .any(|b| matches!(b, Block::Heading { level: 2, .. })),
            "{blocks:?}"
        );
        assert!(
            blocks.iter().any(|b| matches!(b, Block::Table { .. })),
            "table missing: {blocks:?}"
        );
        assert!(
            blocks
                .iter()
                .any(|b| matches!(b, Block::List { ordered: true, .. })),
            "ordered list missing: {blocks:?}"
        );
        assert!(
            blocks
                .iter()
                .any(|b| matches!(b, Block::CodeBlock { lang, .. } if lang == "json")),
            "fenced code missing: {blocks:?}"
        );

        let table = blocks
            .iter()
            .find_map(|b| match b {
                Block::Table { headers, rows } => Some((headers, rows)),
                _ => None,
            })
            .expect("table");
        assert_eq!(table.0.len(), 2);
        assert_eq!(table.1.len(), 2);
        assert!(matches!(table.1[0][0][0], Inline::Strong(_)));
        assert!(matches!(table.1[1][1][0], Inline::Code(_)));
    }

    #[test]
    fn settled_flatten_does_not_leak_source_markers() {
        let blocks = parse_markdown(FIXTURE);
        let plain = flatten_blocks(&blocks);
        assert!(!plain.contains("**"), "{plain}");
        assert!(!plain.contains("|---|"), "{plain}");
        assert!(!plain.contains("## "), "{plain}");
        assert!(plain.contains("文档结构概览"), "{plain}");
        assert!(plain.contains("ck 数据库"), "{plain}");
        assert!(plain.contains("aids_bigdata"), "{plain}");
        assert!(plain.contains("敏感信息"), "{plain}");
    }

    #[test]
    fn italic_and_inline_code_are_structured() {
        let blocks = parse_markdown("这是 *斜体* 和 `行内码`。");
        let Block::Paragraph { spans } = &blocks[0] else {
            panic!("{blocks:?}");
        };
        assert!(spans.iter().any(|s| matches!(s, Inline::Emphasis(_))));
        assert!(spans
            .iter()
            .any(|s| matches!(s, Inline::Code(c) if c == "行内码")));
        assert!(!flatten_blocks(&blocks).contains('*'));
    }

    #[test]
    fn incomplete_streaming_table_does_not_panic() {
        let blocks = parse_markdown("| 章节 | 内容要点 |\n| ---");
        assert!(!blocks.is_empty());
    }

    /// 复现助手气泡的横向父布局：块必须上下堆叠，表格单元格仍是网格。
    #[test]
    fn multiblock_markdown_stacks_under_horizontal_parent() {
        let ctx = egui::Context::default();
        ctx.set_fonts(egui::FontDefinitions::default());
        let raw = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::pos2(0.0, 0.0),
                egui::vec2(520.0, 900.0),
            )),
            ..Default::default()
        };
        let output = ctx.run(raw, |ctx| {
            egui::CentralPanel::default()
                .frame(egui::Frame::none())
                .show(ctx, |ui| {
                    ui.set_width(480.0);
                    // 与修复前 show_assistant_message 相同的方向：左对齐被误写成横向流。
                    ui.with_layout(egui::Layout::left_to_right(egui::Align::TOP), |ui| {
                        ui.set_max_width(480.0);
                        show_markdown(
                            ui,
                            FIXTURE,
                            MarkdownView {
                                streaming: false,
                                show_role: true,
                            },
                        );
                    });
                });
        });

        let mut texts = Vec::new();
        let mut rects = Vec::new();
        for clipped in &output.shapes {
            collect_shapes(&clipped.shape, &mut texts, &mut rects);
        }
        let heading = text_pos(&texts, "文档结构概览");
        let col_a = text_pos(&texts, "章节");
        let col_b = text_pos(&texts, "内容要点");
        let reminder = text_pos(&texts, "几点提醒");
        let list_item = text_pos(&texts, "敏感信息");
        let code = text_pos(&texts, "ok");
        let tail = text_pos(&texts, "需要我继续");
        let kafka = text_pos(&texts, "Kafka");

        assert!(
            col_a.y > heading.y + 12.0,
            "table should sit below the heading, heading={heading:?} col={col_a:?}"
        );
        assert!(
            (col_a.y - col_b.y).abs() < 8.0 && col_b.x > col_a.x + 16.0,
            "header cells should share a row, a={col_a:?} b={col_b:?}"
        );
        assert!(
            kafka.y > col_a.y + 8.0 && (kafka.x - col_a.x).abs() < 24.0,
            "body cell should stack under its header, header={col_a:?} kafka={kafka:?}"
        );
        assert!(reminder.y > col_a.y + 8.0, "second heading below table");
        assert!(list_item.y > reminder.y + 8.0, "list below second heading");
        assert!(
            code.y > list_item.y + 8.0 && (code.x - heading.x).abs() < 80.0,
            "fenced code should stack in the same column, heading={heading:?} code={code:?}"
        );
        assert!(tail.y > code.y + 8.0, "paragraph below code");
        assert!(
            texts.iter().any(|(text, _)| text.contains("助手")),
            "role chip missing: {texts:?}"
        );
        let code_well = rects.iter().any(|rect| {
            rect.contains(code) && rect.width() > 80.0 && rect.height() > 16.0 && rect.left() < 80.0
        });
        assert!(
            code_well,
            "code fence frame missing or clipped off the column, code={code:?} rects={rects:?}"
        );
    }

    fn text_pos(texts: &[(String, egui::Pos2)], needle: &str) -> egui::Pos2 {
        texts
            .iter()
            .find(|(text, _)| text.contains(needle))
            .unwrap_or_else(|| panic!("missing {needle} in {texts:?}"))
            .1
    }

    fn collect_shapes(
        shape: &egui::Shape,
        texts: &mut Vec<(String, egui::Pos2)>,
        rects: &mut Vec<egui::Rect>,
    ) {
        match shape {
            egui::Shape::Text(text) => {
                texts.push((text.galley.text().to_owned(), text.pos));
            }
            egui::Shape::Rect(rect) => rects.push(rect.rect),
            egui::Shape::Vec(shapes) => {
                for shape in shapes {
                    collect_shapes(shape, texts, rects);
                }
            }
            _ => {}
        }
    }
}
