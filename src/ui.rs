//! How the UI is rendered, with the Ratatui library.

use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
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

const PINK: Color = Color::Rgb(255, 150, 167);

// wrap text to fit within a given width, splitting on word boundaries when possible
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

        if test_line.len() <= width {
            current_line = test_line;
        } else {
            if !current_line.is_empty() {
                lines.push(current_line);
            }
            // if a single word is longer than width, split it
            if word.len() > width {
                let mut remaining = word;
                while remaining.len() > width {
                    lines.push(remaining[..width].to_string());
                    remaining = &remaining[width..];
                }
                current_line = remaining.to_string();
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

pub fn predraw(f: &Frame) -> Rc<[Rect]> {
    Layout::default()
        .constraints([Constraint::Percentage(30), Constraint::Percentage(70)].as_ref())
        .direction(Direction::Horizontal)
        .split(f.area())
}

pub fn draw(f: &mut Frame, chunks: Rc<[Rect]>, app: &mut AppImpl) {
    draw_info_column(f, chunks[0], app);

    match &app.selected {
        Selected::Feeds | Selected::Entries => {
            draw_entries(f, chunks[1], app);
        }
        Selected::Entry(_entry_meta) => {
            draw_entry(f, chunks[1], app);
        }
        Selected::None => draw_entries(f, chunks[1], app),
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
            Selected::Entry(entry) => draw_entry_info(f, chunks[1], entry),
            Selected::Entries => {
                if let Some(entry_meta) = &app.current_entry_meta {
                    draw_entry_info(f, chunks[1], entry_meta);
                } else {
                    draw_feed_info(f, chunks[1], app);
                }
            }
            Selected::None => draw_first_run_helper(f, chunks[1]),
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

fn draw_first_run_helper(f: &mut Frame, area: Rect) {
    let text = "Press 'i', then enter an RSS/Atom feed URL, then hit `Enter`!";

    let block = Block::default().borders(Borders::ALL).title(Span::styled(
        "TO SUBSCRIBE TO YOUR FIRST FEED",
        Style::default().fg(PINK).add_modifier(Modifier::BOLD),
    ));

    let paragraph = Paragraph::new(Text::from(text))
        .block(block)
        .wrap(Wrap { trim: false });

    f.render_widget(paragraph, area);
}

fn draw_entry_info(f: &mut Frame, area: Rect, entry_meta: &EntryMetadata) {
    let mut text = String::new();
    if let Some(item) = &entry_meta.title {
        text.push_str("Title: ");
        text.push_str(item.to_string().as_str());
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

    let block = Block::default().borders(Borders::ALL).title(Span::styled(
        "Info",
        Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD),
    ));

    let paragraph = Paragraph::new(Text::from(text.as_str()))
        .block(block)
        .wrap(Wrap { trim: false });

    f.render_widget(paragraph, area);
}

/// Renders activity data as a mini bar chart using Unicode block characters
/// Returns a styled span for better visual appearance
fn render_mini_sparkline(data: &[u64]) -> Span<'static> {
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

    // use a muted cyan-gray color that complements the UI theme
    Span::styled(sparkline_text, Style::default().fg(Color::Rgb(120, 150, 160)))
}

fn draw_feeds(f: &mut Frame, area: Rect, app: &mut AppImpl) {
    // create feed list items with unread counts and sparklines
    let feeds: Vec<ListItem> = app
        .feeds
        .items
        .iter()
        .map(|feed| {
            let feed_title = feed.title.as_deref().unwrap_or("No title");

            // get unread count for this feed
            let unread_count = crate::rss::count_unread_entries(&app.conn, feed.id).unwrap_or(0);

            // build the display with styled components
            let mut display_spans = vec![Span::raw(feed_title)];
            
            // add sparkline if available
            if let Some(data) = app.feed_activity_cache.get(&feed.id) {
                if !data.is_empty() {
                    display_spans.push(Span::raw(" "));
                    display_spans.push(render_mini_sparkline(data));
                }
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
                .style(Style::default().fg(Color::Yellow))
                .wrap(Wrap { trim: false });
            f.render_widget(flash_paragraph, chunks[0]);
        }

        // show feeds list with normal title
        let feeds = List::new(feeds).block(
            Block::default().borders(Borders::ALL).title(Span::styled(
                &default_title,
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            )),
        );

        let feeds = match app.selected {
            Selected::Feeds => feeds
                .highlight_style(Style::default().fg(PINK).add_modifier(Modifier::BOLD))
                .highlight_symbol("> "),
            _ => feeds,
        };

        f.render_stateful_widget(feeds, chunks[1], &mut app.feeds.state);
    } else {
        // no flash message, show feeds list normally
        let feeds = List::new(feeds).block(
            Block::default().borders(Borders::ALL).title(Span::styled(
                &default_title,
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            )),
        );

        let feeds = match app.selected {
            Selected::Feeds => feeds
                .highlight_style(Style::default().fg(PINK).add_modifier(Modifier::BOLD))
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
        text.push_str(item);
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

    let block = Block::default().borders(Borders::ALL).title(Span::styled(
        "Info",
        Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD),
    ));

    let paragraph = Paragraph::new(Text::from(text.as_str()))
        .block(block)
        .wrap(Wrap { trim: false });

    f.render_widget(paragraph, area);
}

fn draw_help(f: &mut Frame, area: Rect, app: &mut AppImpl) {
    let mut text = String::new();
    match app.selected {
        Selected::Feeds => {
            text.push_str("r - refresh selected feed; x - refresh all feeds\n");
            text.push_str("c - copy link; o - open link in browser\n");
            if app.mode == Mode::Normal {
                text.push_str("d - delete feed (with confirmation)\n");
                text.push_str("E - export feeds to OPML\n");
                text.push_str("e/i - edit mode\n");
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

    text.push_str("? - show/hide help");

    let help_message =
        Paragraph::new(Text::from(text.as_str())).block(Block::default().borders(Borders::ALL));
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

    let input = Paragraph::new(text)
        .style(Style::default().fg(Color::Yellow))
        .block(
            Block::default().borders(Borders::ALL).title(Span::styled(
                title,
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            )),
        );
    f.render_widget(input, area);
}

fn draw_tabs(f: &mut Frame, area: Rect, app: &AppImpl) {
    let titles = vec![" Unread ", " All ", " Read "];
    let selected_idx = match app.read_mode {
        ReadMode::ShowUnread => 0,
        ReadMode::All => 1,
        ReadMode::ShowRead => 2,
    };

    let tabs = Tabs::new(titles)
        .style(Style::default().fg(Color::White))
        .highlight_style(Style::default().fg(PINK).add_modifier(Modifier::BOLD))
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

    // calculate available width for wrapping (accounting for borders and highlight symbol)
    let available_width = if entries_area.width > 4 {
        entries_area.width as usize - 4
    } else {
        1
    };

    let entries = app
        .entries
        .items
        .iter()
        .map(|entry| {
            let title_text = entry
                .title
                .as_ref()
                .map_or_else(|| "No title".to_string(), |t| t.to_string());

            // wrap the title text to fit the available width
            let wrapped_lines = wrap_text(&title_text, available_width);

            // create a list item with multiple lines if needed
            if wrapped_lines.len() == 1 {
                ListItem::new(Span::raw(wrapped_lines[0].clone()))
            } else {
                // create multiple lines for multi-line items
                let lines: Vec<Line> = wrapped_lines
                    .into_iter()
                    .map(|line| Line::from(Span::raw(line)))
                    .collect();
                ListItem::new(Text::from(lines))
            }
        })
        .collect::<Vec<ListItem>>();

    let default_title = "Entries".to_string();

    let title = app
        .current_feed
        .as_ref()
        .and_then(|feed| feed.title.as_ref())
        .unwrap_or(&default_title);

    let entries_titles = List::new(entries).block(
        Block::default().borders(Borders::ALL).title(Span::styled(
            title,
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        )),
    );

    let entries_titles = match app.selected {
        Selected::Entries => entries_titles
            .highlight_style(Style::default().fg(PINK).add_modifier(Modifier::BOLD))
            .highlight_symbol("> "),
        _ => entries_titles,
    };

    if !&app.error_flash.is_empty() {
        let error_chunks = Layout::default()
            .constraints([Constraint::Percentage(60), Constraint::Percentage(40)].as_ref())
            .direction(Direction::Vertical)
            .split(entries_area);

        let error_text = error_text(&app.error_flash);

        let block = Block::default().borders(Borders::ALL).title(Span::styled(
            "Error - press 'q' to close",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ));

        let error_widget = Paragraph::new(error_text)
            .block(block)
            .wrap(Wrap { trim: false })
            .scroll((0, 0));

        f.render_stateful_widget(entries_titles, error_chunks[0], &mut app.entries.state);
        f.render_widget(error_widget, error_chunks[1]);
    } else {
        f.render_stateful_widget(entries_titles, entries_area, &mut app.entries.state);
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

    let entry_title = entry_meta.title.as_deref().unwrap_or("No entry title");

    let feed_title = app
        .current_feed
        .as_ref()
        .and_then(|feed| feed.title.as_deref())
        .unwrap_or("No feed title");

    let mut title = String::new();
    title.reserve_exact(entry_title.len() + feed_title.len() + 3);
    title.push_str(entry_title);
    title.push_str(" - ");
    title.push_str(feed_title);

    let block = Block::default().borders(Borders::ALL).title(Span::styled(
        &title,
        Style::default()
            .add_modifier(Modifier::BOLD)
            .fg(Color::Cyan),
    ));

    let paragraph = Paragraph::new(app.current_entry_text.as_str())
        .block(block)
        .wrap(Wrap { trim: false })
        .scroll((scroll, 0));

    // Calculate visible lines for scrolling (account for borders and tabs)
    let entry_chunk_height = content_area.height.saturating_sub(2);
    app.entry_lines_rendered_len = entry_chunk_height;

    // Create scrollbar
    let scrollbar = Scrollbar::default()
        .orientation(ScrollbarOrientation::VerticalRight)
        .thumb_style(Style::default().fg(PINK))
        .track_style(Style::default().fg(Color::DarkGray));

    let mut scrollbar_state = ScrollbarState::new(app.entry_lines_len)
        .position(app.entry_scroll_position as usize);

    if !app.error_flash.is_empty() {
        let chunks = Layout::default()
            .constraints([Constraint::Percentage(60), Constraint::Percentage(40)].as_ref())
            .direction(Direction::Vertical)
            .split(content_area);

        let error_text = error_text(&app.error_flash);
        let error_block = Block::default().borders(Borders::ALL).title(Span::styled(
            "Error - press 'q' to close",
            Style::default()
                .add_modifier(Modifier::BOLD)
                .fg(Color::Cyan),
        ));

        let error_widget = Paragraph::new(error_text)
            .block(error_block)
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
