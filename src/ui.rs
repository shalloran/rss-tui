// UI rendering with Ratatui

use ratatui::Frame;
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{
    Block, Borders, List, ListItem, Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState,
    Tabs, Wrap,
};
use std::rc::Rc;

use crate::app::AppImpl;
use crate::modes::{Mode, ReadMode, Selected};
use crate::rss::EntryMetadata;
use crate::util::sanitize_for_display;
use chrono::Utc;
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

const PINK: Color = Color::Rgb(255, 150, 167);

// theme system
#[derive(Clone, Copy, Debug)]
pub enum Theme {
    Boring,
    Hacker,
    Ubuntu,
}

impl Theme {
    pub fn unread_entry_color(&self) -> Color {
        match self {
            Theme::Boring => Color::Yellow,
            Theme::Hacker => Color::Rgb(0, 255, 0), // bright green
            Theme::Ubuntu => Color::Rgb(255, 140, 0), // orange
        }
    }

    pub fn read_entry_color(&self) -> Color {
        match self {
            Theme::Boring => Color::DarkGray,
            Theme::Hacker => Color::Rgb(0, 150, 0), // darker green
            Theme::Ubuntu => Color::DarkGray,
        }
    }

    pub fn new_entry_color(&self) -> Color {
        match self {
            Theme::Boring => Color::Green,
            Theme::Hacker => Color::Cyan,
            Theme::Ubuntu => Color::Rgb(119, 41, 83), // purple
        }
    }

    pub fn unread_feed_color(&self) -> Color {
        match self {
            Theme::Boring => Color::Yellow,
            Theme::Hacker => Color::Rgb(0, 255, 0), // bright green
            Theme::Ubuntu => Color::Rgb(255, 140, 0), // orange
        }
    }

    pub fn error_color(&self) -> Color {
        match self {
            Theme::Boring => Color::Red,
            Theme::Hacker => Color::Rgb(255, 0, 0), // bright red
            Theme::Ubuntu => Color::Red,
        }
    }

    pub fn feed_type_badge_color(&self) -> Color {
        match self {
            Theme::Boring => Color::DarkGray,
            Theme::Hacker => Color::Rgb(0, 200, 0), // medium green
            Theme::Ubuntu => Color::DarkGray,
        }
    }

    // background color for the entire UI
    pub fn background_color(&self) -> Color {
        match self {
            Theme::Boring => Color::Reset,
            Theme::Hacker => Color::Black,
            Theme::Ubuntu => Color::Reset,
        }
    }

    // default text color
    pub fn text_color(&self) -> Color {
        match self {
            Theme::Boring => Color::Reset,
            Theme::Hacker => Color::Rgb(0, 255, 0), // bright green
            Theme::Ubuntu => Color::Reset,
        }
    }

    // title/header color
    pub fn title_color(&self) -> Color {
        match self {
            Theme::Boring => Color::Cyan,
            Theme::Hacker => Color::Rgb(0, 255, 255), // bright cyan
            Theme::Ubuntu => Color::Cyan,
        }
    }

    // border color
    pub fn border_color(&self) -> Color {
        match self {
            Theme::Boring => Color::Reset,
            Theme::Hacker => Color::Rgb(0, 200, 0), // medium green
            Theme::Ubuntu => Color::Reset,
        }
    }

    // highlight/selection color
    pub fn highlight_color(&self) -> Color {
        match self {
            Theme::Boring => PINK,
            Theme::Hacker => Color::Rgb(0, 255, 255), // bright cyan
            Theme::Ubuntu => PINK,
        }
    }

    // flash message color
    pub fn flash_color(&self) -> Color {
        match self {
            Theme::Boring => Color::Yellow,
            Theme::Hacker => Color::Rgb(0, 255, 0), // bright green
            Theme::Ubuntu => Color::Yellow,
        }
    }

    // command bar text (hacker: black on green bar for contrast)
    pub fn command_bar_text_color(&self) -> Color {
        match self {
            Theme::Hacker => Color::Black,
            _ => self.text_color(),
        }
    }
}

// symbols configuration
#[derive(Clone, Debug)]
pub struct Symbols {
    pub unread_entry: &'static str,
    pub read_entry: &'static str,
    pub new_entry: &'static str,
    pub unread_feed: &'static str,
    pub error: &'static str,
    pub feed_type_rss: &'static str,
    pub feed_type_atom: &'static str,
}

impl Default for Symbols {
    fn default() -> Self {
        Symbols {
            unread_entry: "● ",
            read_entry: "✓ ",
            new_entry: "🆕 ",
            unread_feed: "● ",
            error: "⚠ ",
            feed_type_rss: " [RSS]",
            feed_type_atom: " [ATOM]",
        }
    }
}

// get current theme from app state
fn get_theme(app: &AppImpl) -> Theme {
    app.current_theme
}

// get current symbols (for now, default; can be made configurable later)
fn get_symbols() -> Symbols {
    Symbols::default()
}

// wrap text to fit within a given display width, splitting on word boundaries when possible
fn wrap_text(text: &str, width: usize) -> Vec<String> {
    if width == 0 {
        return vec![];
    }

    let mut lines = Vec::new();
    let words: Vec<&str> = text.split_whitespace().collect();

    if words.is_empty() {
        return vec![text.to_string()];
    }

    let mut current_line = String::new();

    for word in words {
        let test_line = if current_line.is_empty() {
            word.to_string()
        } else {
            format!("{} {}", current_line, word)
        };

        if test_line.width() <= width {
            current_line = test_line;
        } else {
            if !current_line.is_empty() {
                lines.push(current_line);
            }
            // split long word by display width at char boundaries
            if word.width() > width {
                let mut segment = String::new();
                let mut seg_width = 0usize;
                for c in word.chars() {
                    let cw = c.width().unwrap_or(0);
                    if seg_width + cw > width && !segment.is_empty() {
                        lines.push(std::mem::take(&mut segment));
                        seg_width = 0;
                    }
                    seg_width += cw;
                    segment.push(c);
                }
                current_line = segment;
            } else {
                current_line = word.to_string();
            }
        }
    }

    if !current_line.is_empty() {
        lines.push(current_line);
    }

    if lines.is_empty() {
        vec![text.to_string()]
    } else {
        lines
    }
}

// command bar: 1 line normally, 2 when text wraps on narrow terminals
fn command_bar_height(f: &Frame, app: &AppImpl) -> u16 {
    let line = command_bar_line(app);
    if line.width() as u16 <= f.area().width {
        1
    } else {
        2
    }
}

pub fn predraw(f: &Frame, app: &AppImpl) -> Rc<[Rect]> {
    let bar_height = command_bar_height(f, app);
    let vertical = Layout::default()
        .constraints([Constraint::Min(0), Constraint::Length(bar_height)].as_ref())
        .direction(Direction::Vertical)
        .split(f.area());
    let main_area = vertical[0];
    let bottom_bar = vertical[1];
    let horizontal = Layout::default()
        .constraints([Constraint::Percentage(30), Constraint::Percentage(70)].as_ref())
        .direction(Direction::Horizontal)
        .split(main_area);
    // [left, right, command_bar]
    let mut out = horizontal.to_vec();
    out.push(bottom_bar);
    out.into()
}

pub fn draw(f: &mut Frame, chunks: Rc<[Rect]>, app: &mut AppImpl) {
    draw_info_column(f, chunks[0], app);

    match &app.selected {
        Selected::Feeds | Selected::Entries => {
            draw_entries(f, chunks[1], app);
        }
        Selected::CombinedUnread => {
            draw_combined_entries(f, chunks[1], app);
        }
        Selected::Entry(_entry_meta) => {
            draw_entry(f, chunks[1], app);
        }
        Selected::None => draw_entries(f, chunks[1], app),
    }

    if chunks.len() >= 3 {
        draw_command_bar(f, chunks[2], app);
    }
}

fn draw_info_column(f: &mut Frame, area: Rect, app: &mut AppImpl) {
    let mut constraints = match &app.mode {
        Mode::Normal => vec![Constraint::Percentage(70), Constraint::Percentage(30)],
        Mode::Editing => vec![
            Constraint::Percentage(60),
            Constraint::Percentage(20),
            Constraint::Percentage(10),
        ],
    };

    if app.show_help {
        constraints[1] = Constraint::Percentage(20);
        constraints.push(Constraint::Percentage(10));
    }

    let chunks = Layout::default()
        .constraints(constraints)
        .direction(Direction::Vertical)
        .split(area);
    {
        // FEEDS
        draw_feeds(f, chunks[0], app);

        // INFO
        match &app.selected {
            Selected::Entry(entry) => draw_entry_info(f, chunks[1], entry, app),
            Selected::Entries | Selected::CombinedUnread => {
                if let Some(entry_meta) = &app.current_entry_meta {
                    draw_entry_info(f, chunks[1], entry_meta, app);
                } else {
                    draw_feed_info(f, chunks[1], app);
                }
            }
            Selected::None => draw_first_run_helper(f, chunks[1], app),
            _ => {
                if app.current_feed.is_some() {
                    draw_feed_info(f, chunks[1], app);
                }
            }
        }

        match (app.mode, app.show_help) {
            (Mode::Editing, true) => {
                draw_new_feed_input(f, chunks[2], app);
                draw_help(f, chunks[3], app);
            }
            (Mode::Editing, false) => {
                draw_new_feed_input(f, chunks[2], app);
            }
            (_, true) => {
                draw_help(f, chunks[2], app);
            }
            _ => (),
        }
    }
}

fn draw_first_run_helper(f: &mut Frame, area: Rect, app: &AppImpl) {
    let theme = get_theme(app);
    let text = "Press 'i', then enter an RSS/Atom feed URL, then hit `Enter`!";

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme.border_color()))
        .style(Style::default().bg(theme.background_color()))
        .title(Span::styled(
            "TO SUBSCRIBE TO YOUR FIRST FEED",
            Style::default()
                .fg(theme.highlight_color())
                .bg(theme.background_color())
                .add_modifier(Modifier::BOLD),
        ));

    let paragraph = Paragraph::new(Text::from(text))
        .block(block)
        .style(
            Style::default()
                .fg(theme.text_color())
                .bg(theme.background_color()),
        )
        .wrap(Wrap { trim: false });

    f.render_widget(paragraph, area);
}

fn draw_entry_info(f: &mut Frame, area: Rect, entry_meta: &EntryMetadata, app: &AppImpl) {
    let theme = get_theme(app);
    let mut text = String::new();
    if let Some(item) = &entry_meta.title {
        text.push_str("Title: ");
        text.push_str(&sanitize_for_display(item));
        text.push('\n');
    };

    if let Some(item) = &entry_meta.link {
        text.push_str("Link: ");
        text.push_str(item);
        text.push('\n');
    }

    if let Some(pub_date) = &entry_meta.pub_date {
        text.push_str("Pub. date: ");
        text.push_str(pub_date.to_string().as_str());
    } else {
        // TODO this should probably pull the <updated> tag
        // and use that
        let inserted_at = entry_meta.inserted_at;
        text.push_str("Pulled date: ");
        text.push_str(inserted_at.to_string().as_str());
    }
    text.push('\n');

    if let Some(read_at) = &entry_meta.read_at {
        text.push_str("Read at: ");
        text.push_str(read_at.to_string().as_str());
        text.push('\n');
    }

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme.border_color()))
        .style(Style::default().bg(theme.background_color()))
        .title(Span::styled(
            "Info",
            Style::default()
                .fg(theme.title_color())
                .bg(theme.background_color())
                .add_modifier(Modifier::BOLD),
        ));

    let paragraph = Paragraph::new(Text::from(text.as_str()))
        .block(block)
        .style(
            Style::default()
                .fg(theme.text_color())
                .bg(theme.background_color()),
        )
        .wrap(Wrap { trim: false });

    f.render_widget(paragraph, area);
}

/// Renders activity data as a mini bar chart using Unicode block characters
/// Returns a styled span for better visual appearance
fn render_mini_sparkline(data: &[u64], theme: Theme) -> Span<'static> {
    // use smoother block characters for better visual appearance
    const BARS: [char; 8] = [' ', '▁', '▂', '▃', '▄', '▅', '▆', '▇'];

    if data.is_empty() {
        return Span::raw("");
    }

    let max = *data.iter().max().unwrap_or(&1);
    if max == 0 {
        return Span::styled(
            data.iter().map(|_| BARS[0]).collect::<String>(),
            Style::default().fg(Color::DarkGray),
        );
    }

    let sparkline_text: String = data
        .iter()
        .map(|&v| {
            let idx = ((v * 7) / max) as usize;
            BARS[idx.min(7)]
        })
        .collect();

    // use theme-appropriate color for sparkline
    let sparkline_color = match theme {
        Theme::Boring => Color::Rgb(120, 150, 160), // muted cyan-gray
        Theme::Hacker => Color::Rgb(0, 200, 0),     // medium green
        Theme::Ubuntu => Color::Rgb(120, 150, 160), // muted cyan-gray
    };
    Span::styled(
        sparkline_text,
        Style::default()
            .fg(sparkline_color)
            .bg(theme.background_color()),
    )
}

fn draw_feeds(f: &mut Frame, area: Rect, app: &mut AppImpl) {
    let theme = get_theme(app);
    let symbols = get_symbols();

    // create feed list items with unread counts and sparklines
    let feeds: Vec<ListItem> = app
        .feeds
        .items
        .iter()
        .map(|feed| {
            let feed_title = sanitize_for_display(feed.title.as_deref().unwrap_or("No title"));

            // get unread count for this feed
            let unread_count = crate::rss::count_unread_entries(&app.conn, feed.id).unwrap_or(0);

            // build the display with styled components
            let mut display_spans = Vec::new();

            // unread status prefix
            if unread_count > 0 {
                display_spans.push(Span::styled(
                    symbols.unread_feed,
                    Style::default().fg(theme.unread_feed_color()),
                ));
            } else {
                display_spans.push(Span::raw("  ")); // spacing for alignment
            }

            // feed title
            display_spans.push(Span::raw(feed_title));

            // feed type badge
            let feed_type_badge = match feed.feed_kind {
                crate::rss::FeedKind::Rss => symbols.feed_type_rss,
                crate::rss::FeedKind::Atom => symbols.feed_type_atom,
            };
            display_spans.push(Span::styled(
                feed_type_badge,
                Style::default().fg(theme.feed_type_badge_color()),
            ));

            // error indicator
            if app.feed_errors.contains_key(&feed.id) {
                display_spans.push(Span::raw(" "));
                display_spans.push(Span::styled(
                    symbols.error,
                    Style::default().fg(theme.error_color()),
                ));
            }

            // add sparkline if available
            if let Some(data) = app.feed_activity_cache.get(&feed.id)
                && !data.is_empty()
            {
                display_spans.push(Span::raw(" "));
                display_spans.push(render_mini_sparkline(data, theme));
            }

            // add unread count if > 0
            if unread_count > 0 {
                display_spans.push(Span::raw(" "));
                display_spans.push(Span::styled(
                    format!("({})", unread_count),
                    Style::default().fg(Color::DarkGray),
                ));
            }

            ListItem::new(Line::from(display_spans))
        })
        .collect();

    let default_title = String::from("Feeds");

    // if there's a flash message, split the area to show it separately
    if app.flash.is_some() {
        let chunks = Layout::default()
            .constraints([Constraint::Length(3), Constraint::Min(0)].as_ref())
            .direction(Direction::Vertical)
            .split(area);

        // show flash message in a paragraph at the top
        if let Some(flash_text) = &app.flash {
            let flash_paragraph = Paragraph::new(Text::from(flash_text.as_str()))
                .style(
                    Style::default()
                        .fg(theme.flash_color())
                        .bg(theme.background_color()),
                )
                .wrap(Wrap { trim: false });
            f.render_widget(flash_paragraph, chunks[0]);
        }

        // show feeds list with normal title
        let feeds = List::new(feeds).block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(theme.border_color()))
                .style(Style::default().bg(theme.background_color()))
                .title(Span::styled(
                    &default_title,
                    Style::default()
                        .fg(theme.title_color())
                        .bg(theme.background_color())
                        .add_modifier(Modifier::BOLD),
                )),
        );

        let feeds = match app.selected {
            Selected::Feeds => feeds
                .highlight_style(
                    Style::default()
                        .fg(theme.highlight_color())
                        .bg(theme.background_color())
                        .add_modifier(Modifier::BOLD),
                )
                .highlight_symbol("> "),
            _ => feeds,
        };

        f.render_stateful_widget(feeds, chunks[1], &mut app.feeds.state);
    } else {
        // no flash message, show feeds list normally
        let feeds = List::new(feeds).block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(theme.border_color()))
                .style(Style::default().bg(theme.background_color()))
                .title(Span::styled(
                    &default_title,
                    Style::default()
                        .fg(theme.title_color())
                        .bg(theme.background_color())
                        .add_modifier(Modifier::BOLD),
                )),
        );

        let feeds = match app.selected {
            Selected::Feeds => feeds
                .highlight_style(
                    Style::default()
                        .fg(theme.highlight_color())
                        .bg(theme.background_color())
                        .add_modifier(Modifier::BOLD),
                )
                .highlight_symbol("> "),
            _ => feeds,
        };

        f.render_stateful_widget(feeds, area, &mut app.feeds.state);
    }
}

fn draw_feed_info(f: &mut Frame, area: Rect, app: &mut AppImpl) {
    let mut text = String::new();
    if let Some(item) = app
        .current_feed
        .as_ref()
        .and_then(|feed| feed.title.as_ref())
    {
        text.push_str("Title: ");
        text.push_str(&sanitize_for_display(item));
        text.push('\n');
    }

    if let Some(item) = app
        .current_feed
        .as_ref()
        .and_then(|feed| feed.link.as_ref())
    {
        text.push_str("Link: ");
        text.push_str(item);
        text.push('\n');
    }

    if let Some(item) = app
        .current_feed
        .as_ref()
        .and_then(|feed| feed.feed_link.as_ref())
    {
        text.push_str("Feed link: ");
        text.push_str(item);
        text.push('\n');
    }

    if let Some(item) = app.entries.items.first()
        && let Some(pub_date) = &item.pub_date
    {
        text.push_str("Most recent entry at: ");
        text.push_str(pub_date.to_string().as_str());
        text.push('\n');
    }

    if let Some(item) = &app
        .current_feed
        .as_ref()
        .and_then(|feed| feed.refreshed_at)
        .map(|timestamp| timestamp.to_string())
        .or_else(|| Some("Never refreshed".to_string()))
    {
        text.push_str("Refreshed at: ");
        text.push_str(item.as_str());
        text.push('\n');
    }

    match app.read_mode {
        ReadMode::ShowUnread => text.push_str("Unread entries: "),
        ReadMode::ShowRead => text.push_str("Read entries: "),
        ReadMode::All => text.push_str("All entries: "),
    }
    text.push_str(app.entries.items.len().to_string().as_str());
    text.push('\n');

    if let Some(feed_kind) = app.current_feed.as_ref().map(|feed| feed.feed_kind) {
        text.push_str("Feed kind: ");
        text.push_str(&feed_kind.to_string());
        text.push('\n');
    }

    let theme = get_theme(app);
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme.border_color()))
        .style(Style::default().bg(theme.background_color()))
        .title(Span::styled(
            "Info",
            Style::default()
                .fg(theme.title_color())
                .bg(theme.background_color())
                .add_modifier(Modifier::BOLD),
        ));

    let paragraph = Paragraph::new(Text::from(text.as_str()))
        .block(block)
        .style(
            Style::default()
                .fg(theme.text_color())
                .bg(theme.background_color()),
        )
        .wrap(Wrap { trim: false });

    f.render_widget(paragraph, area);
}

/// format one keybinding as vim-style "[ key ] action"
fn cmd(key: &str, action: &str) -> String {
    format!("[ {} ] {}", key, action)
}

/// compact vim-style keybinding summary for the bottom bar (context-aware)
fn command_bar_line(app: &AppImpl) -> String {
    let mut parts = Vec::new();
    match app.selected {
        Selected::Feeds => {
            parts.push(cmd("r", "ref"));
            parts.push(cmd("x", "all"));
            parts.push(cmd("A", "all unread"));
            parts.push(cmd("c", "copy"));
            parts.push(cmd("o", "open"));
            if app.mode == Mode::Normal {
                parts.push(cmd("d", "del"));
                parts.push(cmd("E", "opml"));
                parts.push(cmd("e/i", "edit"));
            }
        }
        Selected::Entry(_) | Selected::Entries | Selected::CombinedUnread => {
            parts.push(cmd("r", "read"));
            parts.push(cmd("a", "tabs"));
            parts.push(cmd("c", "copy"));
            parts.push(cmd("o", "open"));
            if app.mode == Mode::Normal {
                parts.push(cmd("e", "mail"));
                parts.push(cmd("E", "opml"));
            }
        }
        _ => {
            parts.push(cmd("r", "read"));
            parts.push(cmd("a", "tabs"));
            parts.push(cmd("c", "copy"));
            parts.push(cmd("o", "open"));
            if app.mode == Mode::Normal {
                parts.push(cmd("E", "opml"));
            }
        }
    }
    match app.mode {
        Mode::Normal => {
            parts.push(cmd("1/2/3", "tabs"));
            parts.push(cmd("i", "edit"));
            parts.push(cmd("q", "quit"));
            if app.pending_deletion.is_some() {
                parts.push(cmd("d", "confirm"));
                parts.push(cmd("n", "cancel"));
            }
        }
        Mode::Editing => {
            if app.pending_rename.is_some() {
                parts.push(cmd("R", "rename"));
                parts.push(cmd("enter", "confirm"));
            } else {
                parts.push(cmd("R", "rename"));
                parts.push(cmd("enter", "fetch"));
                parts.push(cmd("del", "del"));
            }
            parts.push(cmd("esc", "normal"));
        }
    }
    parts.push(cmd("t", "theme"));
    parts.push(cmd("?", "help"));
    parts.join(" ")
}

fn draw_command_bar(f: &mut Frame, area: Rect, app: &mut AppImpl) {
    let theme = get_theme(app);
    let line = command_bar_line(app);
    let bar = Paragraph::new(Text::from(line.as_str()))
        .style(
            Style::default()
                .fg(theme.command_bar_text_color())
                .bg(theme.border_color()),
        )
        .alignment(Alignment::Center)
        .wrap(Wrap { trim: true });
    f.render_widget(bar, area);
}

fn draw_help(f: &mut Frame, area: Rect, app: &mut AppImpl) {
    let mut text = String::new();
    match app.selected {
        Selected::Feeds => {
            text.push_str("r - refresh selected feed; x - refresh all feeds\n");
            text.push_str("A - combined unread (all feeds in one list)\n");
            text.push_str("c - copy link; o - open link in browser\n");
            if app.mode == Mode::Normal {
                text.push_str("d - delete feed (with confirmation)\n");
                text.push_str("E - export feeds to OPML\n");
                text.push_str("e/i - edit mode\n");
            }
        }
        Selected::CombinedUnread => {
            text.push_str("combined view: all unread entries from every feed\n");
            text.push_str("r - mark entry read; a - cycle tabs\n");
            text.push_str("c - copy link; o - open link in browser\n");
            if app.mode == Mode::Normal {
                text.push_str("e - email article; E - export OPML\n");
            }
        }
        Selected::Entry(_) => {
            text.push_str("r - mark entry read/un; a - cycle tabs\n");
            text.push_str("c - copy link; o - open link in browser\n");
            if app.mode == Mode::Normal {
                text.push_str("e - email article (title as subject, URL as body)\n");
                text.push_str("E - export feeds to OPML\n");
            }
        }
        _ => {
            text.push_str("r - mark entry read/un; a - cycle tabs\n");
            text.push_str("c - copy link; o - open link in browser\n");
            if app.mode == Mode::Normal {
                text.push_str("E - export feeds to OPML\n");
            }
        }
    }
    match app.mode {
        Mode::Normal => {
            text.push_str("1/2/3 - Unread/All/Read tabs\n");
            text.push_str("i - edit mode; q - exit\n");
            if app.pending_deletion.is_some() {
                text.push_str("d - confirm deletion; n - cancel\n");
            }
        }
        Mode::Editing => {
            if app.pending_rename.is_some() {
                text.push_str("R - rename feed; enter - confirm rename\n");
            } else {
                text.push_str("R - rename feed; enter - fetch feed; del - delete feed\n");
            }
            text.push_str("esc - normal mode\n")
        }
    }
    text.push_str("t - cycle theme (hacker/ubuntu/boring)\n");

    text.push_str("? - show/hide help");

    let theme = get_theme(app);
    let help_message = Paragraph::new(Text::from(text.as_str()))
        .style(
            Style::default()
                .fg(theme.text_color())
                .bg(theme.background_color()),
        )
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(theme.border_color()))
                .style(Style::default().bg(theme.background_color())),
        );
    f.render_widget(help_message, area);
}

fn draw_new_feed_input(f: &mut Frame, area: Rect, app: &mut AppImpl) {
    let text = &app.feed_subscription_input;
    let text = Text::from(text.as_str());

    let title = if app.pending_rename.is_some() {
        let feed_title = app
            .feeds
            .items
            .iter()
            .find(|f| Some(f.id) == app.pending_rename)
            .and_then(|f| f.title.as_ref())
            .map(|t| t.as_str())
            .unwrap_or("Unknown feed");
        format!("Rename feed: {}", feed_title)
    } else {
        "Add a feed".to_string()
    };

    let theme = get_theme(app);
    let input = Paragraph::new(text)
        .style(
            Style::default()
                .fg(theme.flash_color())
                .bg(theme.background_color()),
        )
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(theme.border_color()))
                .style(Style::default().bg(theme.background_color()))
                .title(Span::styled(
                    title,
                    Style::default()
                        .fg(theme.title_color())
                        .bg(theme.background_color())
                        .add_modifier(Modifier::BOLD),
                )),
        );
    f.render_widget(input, area);
}

fn draw_tabs(f: &mut Frame, area: Rect, app: &AppImpl) {
    let theme = get_theme(app);
    let titles = vec![" Unread ", " All ", " Read "];
    let selected_idx = match app.read_mode {
        ReadMode::ShowUnread => 0,
        ReadMode::All => 1,
        ReadMode::ShowRead => 2,
    };

    let tabs = Tabs::new(titles)
        .style(
            Style::default()
                .fg(theme.text_color())
                .bg(theme.background_color()),
        )
        .highlight_style(
            Style::default()
                .fg(theme.highlight_color())
                .bg(theme.background_color())
                .add_modifier(Modifier::BOLD),
        )
        .select(selected_idx)
        .divider("|");

    f.render_widget(tabs, area);
}

fn draw_entries(f: &mut Frame, area: Rect, app: &mut AppImpl) {
    // Split area for tabs and entries list
    let chunks = Layout::default()
        .constraints([Constraint::Length(1), Constraint::Min(0)].as_ref())
        .direction(Direction::Vertical)
        .split(area);

    // Draw tabs at the top
    draw_tabs(f, chunks[0], app);

    let entries_area = chunks[1];

    let theme = get_theme(app);
    let symbols = get_symbols();

    // calculate available width for wrapping (accounting for borders, highlight symbol, and indicators)
    // indicators take up ~3-4 chars (symbol + space), so subtract that
    let indicator_width = 4;
    let available_width = if entries_area.width > (4 + indicator_width as u16) {
        (entries_area.width as usize - 4 - indicator_width).max(1)
    } else {
        1
    };

    let entries = app
        .entries
        .items
        .iter()
        .map(|entry| {
            let mut spans = Vec::new();

            // read/unread indicator
            if entry.read_at.is_none() {
                spans.push(Span::styled(
                    symbols.unread_entry,
                    Style::default().fg(theme.unread_entry_color()),
                ));
            } else {
                spans.push(Span::styled(
                    symbols.read_entry,
                    Style::default().fg(theme.read_entry_color()),
                ));
            }

            let title_text =
                sanitize_for_display(entry.title.as_ref().map_or("No title", |t| t.as_str()));

            // recency indicator (new entries <24h old)
            let is_new = if let Some(pub_date) = &entry.pub_date {
                let now = Utc::now();
                let age = now - *pub_date;
                age.num_hours() < 24
            } else {
                false
            };

            if is_new {
                // wrap the title text to fit the available width
                let wrapped_lines = wrap_text(title_text.as_str(), available_width);

                // create a list item with multiple lines if needed
                if wrapped_lines.len() == 1 {
                    spans.push(Span::raw(wrapped_lines[0].clone()));
                    spans.push(Span::styled(
                        format!(" {}", symbols.new_entry),
                        Style::default().fg(theme.new_entry_color()),
                    ));
                    ListItem::new(Line::from(spans))
                } else {
                    // create multiple lines for multi-line items
                    let mut lines: Vec<Line> = Vec::new();
                    for (i, line) in wrapped_lines.iter().enumerate() {
                        if i == 0 {
                            let mut first_line_spans = spans.clone();
                            first_line_spans.push(Span::raw(line.clone()));
                            first_line_spans.push(Span::styled(
                                format!(" {}", symbols.new_entry),
                                Style::default().fg(theme.new_entry_color()),
                            ));
                            lines.push(Line::from(first_line_spans));
                        } else {
                            lines.push(Line::from(Span::raw(line.clone())));
                        }
                    }
                    ListItem::new(Text::from(lines))
                }
            } else {
                // wrap the title text to fit the available width
                let wrapped_lines = wrap_text(title_text.as_str(), available_width);

                // create a list item with multiple lines if needed
                if wrapped_lines.len() == 1 {
                    spans.push(Span::raw(wrapped_lines[0].clone()));
                    ListItem::new(Line::from(spans))
                } else {
                    // create multiple lines for multi-line items
                    let mut lines: Vec<Line> = Vec::new();
                    for (i, line) in wrapped_lines.iter().enumerate() {
                        if i == 0 {
                            let mut first_line_spans = spans.clone();
                            first_line_spans.push(Span::raw(line.clone()));
                            lines.push(Line::from(first_line_spans));
                        } else {
                            lines.push(Line::from(Span::raw(line.clone())));
                        }
                    }
                    ListItem::new(Text::from(lines))
                }
            }
        })
        .collect::<Vec<ListItem>>();

    let title = app
        .current_feed
        .as_ref()
        .and_then(|feed| feed.title.as_ref())
        .map(|t| sanitize_for_display(t.as_str()))
        .unwrap_or_else(|| "Entries".to_string());

    let entries_titles = List::new(entries).block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(theme.border_color()))
            .style(Style::default().bg(theme.background_color()))
            .title(Span::styled(
                title.as_str(),
                Style::default()
                    .fg(theme.title_color())
                    .bg(theme.background_color())
                    .add_modifier(Modifier::BOLD),
            )),
    );

    let entries_titles = match app.selected {
        Selected::Entries => entries_titles
            .highlight_style(
                Style::default()
                    .fg(theme.highlight_color())
                    .bg(theme.background_color())
                    .add_modifier(Modifier::BOLD),
            )
            .highlight_symbol("> "),
        _ => entries_titles,
    };

    if !&app.error_flash.is_empty() {
        let error_chunks = Layout::default()
            .constraints([Constraint::Percentage(60), Constraint::Percentage(40)].as_ref())
            .direction(Direction::Vertical)
            .split(entries_area);

        let error_text = error_text(&app.error_flash);

        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(theme.border_color()))
            .style(Style::default().bg(theme.background_color()))
            .title(Span::styled(
                "Error - press 'q' to close",
                Style::default()
                    .fg(theme.title_color())
                    .bg(theme.background_color())
                    .add_modifier(Modifier::BOLD),
            ));

        let error_widget = Paragraph::new(error_text)
            .block(block)
            .style(
                Style::default()
                    .fg(theme.error_color())
                    .bg(theme.background_color()),
            )
            .wrap(Wrap { trim: false })
            .scroll((0, 0));

        f.render_stateful_widget(entries_titles, error_chunks[0], &mut app.entries.state);
        f.render_widget(error_widget, error_chunks[1]);
    } else {
        f.render_stateful_widget(entries_titles, entries_area, &mut app.entries.state);
    }
}

fn draw_combined_entries(f: &mut Frame, area: Rect, app: &mut AppImpl) {
    let chunks = Layout::default()
        .constraints([Constraint::Length(1), Constraint::Min(0)].as_ref())
        .direction(Direction::Vertical)
        .split(area);

    draw_tabs(f, chunks[0], app);
    let entries_area = chunks[1];

    let theme = get_theme(app);
    let symbols = get_symbols();
    let indicator_width = 4;
    let available_width = if entries_area.width > (4 + indicator_width as u16) {
        (entries_area.width as usize - 4 - indicator_width).max(1)
    } else {
        1
    };

    let entries: Vec<ListItem> = app
        .combined_entries
        .items
        .iter()
        .map(|(feed_name, entry)| {
            let mut spans = Vec::new();
            spans.push(Span::styled(
                symbols.unread_entry,
                Style::default().fg(theme.unread_entry_color()),
            ));
            let line_prefix = format!("[{}]: ", sanitize_for_display(feed_name.as_str()));
            let title_text =
                sanitize_for_display(entry.title.as_ref().map_or("No title", |t| t.as_str()));
            let full_text = format!("{}{}", line_prefix, title_text);
            let wrapped_lines = wrap_text(&full_text, available_width);
            if wrapped_lines.len() == 1 {
                spans.push(Span::raw(wrapped_lines[0].clone()));
                ListItem::new(Line::from(spans))
            } else {
                let mut lines: Vec<Line> = Vec::new();
                for (i, line) in wrapped_lines.iter().enumerate() {
                    if i == 0 {
                        let mut first_line_spans = spans.clone();
                        first_line_spans.push(Span::raw(line.clone()));
                        lines.push(Line::from(first_line_spans));
                    } else {
                        lines.push(Line::from(Span::raw(line.clone())));
                    }
                }
                ListItem::new(Text::from(lines))
            }
        })
        .collect();

    let list = List::new(entries).block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(theme.border_color()))
            .style(Style::default().bg(theme.background_color()))
            .title(Span::styled(
                format!("All unread [{}]", app.combined_entries.items.len()),
                Style::default()
                    .fg(theme.title_color())
                    .bg(theme.background_color())
                    .add_modifier(Modifier::BOLD),
            )),
    );

    let list = match app.selected {
        Selected::CombinedUnread => list
            .highlight_style(
                Style::default()
                    .fg(theme.highlight_color())
                    .bg(theme.background_color())
                    .add_modifier(Modifier::BOLD),
            )
            .highlight_symbol("> "),
        _ => list,
    };

    if !app.error_flash.is_empty() {
        let error_chunks = Layout::default()
            .constraints([Constraint::Percentage(60), Constraint::Percentage(40)].as_ref())
            .direction(Direction::Vertical)
            .split(entries_area);
        let error_text = error_text(&app.error_flash);
        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(theme.border_color()))
            .style(Style::default().bg(theme.background_color()))
            .title(Span::styled(
                "Error - press 'q' to close",
                Style::default()
                    .fg(theme.title_color())
                    .bg(theme.background_color())
                    .add_modifier(Modifier::BOLD),
            ));
        let error_widget = Paragraph::new(error_text)
            .block(block)
            .style(
                Style::default()
                    .fg(theme.error_color())
                    .bg(theme.background_color()),
            )
            .wrap(Wrap { trim: false })
            .scroll((0, 0));
        f.render_stateful_widget(list, error_chunks[0], &mut app.combined_entries.state);
        f.render_widget(error_widget, error_chunks[1]);
    } else {
        f.render_stateful_widget(list, entries_area, &mut app.combined_entries.state);
    }
}

fn draw_entry(f: &mut Frame, area: Rect, app: &mut AppImpl) {
    // Split area for tabs and entry content
    let main_chunks = Layout::default()
        .constraints([Constraint::Length(1), Constraint::Min(0)].as_ref())
        .direction(Direction::Vertical)
        .split(area);

    // Draw tabs at the top
    draw_tabs(f, main_chunks[0], app);

    let content_area = main_chunks[1];
    let scroll = app.entry_scroll_position;
    let entry_meta = if let Selected::Entry(e) = &app.selected {
        e
    } else {
        panic!("draw_entry should only be called when app.selected was Selected::Entry")
    };

    let entry_title = sanitize_for_display(entry_meta.title.as_deref().unwrap_or("No entry title"));
    let feed_title = sanitize_for_display(
        app.current_feed
            .as_ref()
            .and_then(|feed| feed.title.as_deref())
            .unwrap_or("No feed title"),
    );

    let mut title = String::new();
    title.reserve_exact(entry_title.len() + feed_title.len() + 3);
    title.push_str(&entry_title);
    title.push_str(" - ");
    title.push_str(&feed_title);

    let theme = get_theme(app);
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme.border_color()))
        .style(Style::default().bg(theme.background_color()))
        .title(Span::styled(
            &title,
            Style::default()
                .fg(theme.title_color())
                .bg(theme.background_color())
                .add_modifier(Modifier::BOLD),
        ));

    let paragraph = Paragraph::new(app.current_entry_text.as_str())
        .block(block)
        .style(
            Style::default()
                .fg(theme.text_color())
                .bg(theme.background_color()),
        )
        .wrap(Wrap { trim: false })
        .scroll((scroll, 0));

    // Calculate visible lines for scrolling (account for borders and tabs)
    let entry_chunk_height = content_area.height.saturating_sub(2);
    app.entry_lines_rendered_len = entry_chunk_height;

    // Create scrollbar
    let scrollbar = Scrollbar::default()
        .orientation(ScrollbarOrientation::VerticalRight)
        .thumb_style(
            Style::default()
                .fg(theme.highlight_color())
                .bg(theme.background_color()),
        )
        .track_style(
            Style::default()
                .fg(theme.border_color())
                .bg(theme.background_color()),
        );

    let mut scrollbar_state =
        ScrollbarState::new(app.entry_lines_len).position(app.entry_scroll_position as usize);

    if !app.error_flash.is_empty() {
        let chunks = Layout::default()
            .constraints([Constraint::Percentage(60), Constraint::Percentage(40)].as_ref())
            .direction(Direction::Vertical)
            .split(content_area);

        let error_text = error_text(&app.error_flash);
        let error_block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(theme.border_color()))
            .style(Style::default().bg(theme.background_color()))
            .title(Span::styled(
                "Error - press 'q' to close",
                Style::default()
                    .fg(theme.title_color())
                    .bg(theme.background_color())
                    .add_modifier(Modifier::BOLD),
            ));

        let error_widget = Paragraph::new(error_text)
            .block(error_block)
            .style(
                Style::default()
                    .fg(theme.error_color())
                    .bg(theme.background_color()),
            )
            .wrap(Wrap { trim: false })
            .scroll((0, 0));

        // Render paragraph and scrollbar in top chunk
        f.render_widget(paragraph, chunks[0]);
        f.render_stateful_widget(scrollbar, chunks[0], &mut scrollbar_state);
        f.render_widget(error_widget, chunks[1]);
    } else {
        // Render paragraph with scrollbar overlay
        f.render_widget(paragraph, content_area);
        f.render_stateful_widget(scrollbar, content_area, &mut scrollbar_state);
    }
}

fn error_text(errors: &[anyhow::Error]) -> String {
    errors
        .iter()
        .flat_map(|e| {
            let mut s = format!("{e:?}")
                .split('\n')
                .map(|s| s.to_owned())
                .collect::<Vec<String>>();
            s.push("\n".to_string());
            s
        })
        .collect::<Vec<String>>()
        .join("\n")
}
