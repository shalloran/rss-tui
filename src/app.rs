//! The main application state is managed here, in `App`.

use crate::modes::{Mode, ReadMode, Selected};
use crate::util;
use anyhow::Result;
use copypasta::{ClipboardContext, ClipboardProvider};
use ratatui::{backend::CrosstermBackend, Terminal};
use std::sync::{Arc, Mutex};

macro_rules! delegate_to_locked_inner {
    ($(($fn_name:ident, $t:ty)),* $(,)? ) => {
        $(
            pub fn $fn_name(&self) -> $t {
                let inner = self.inner.lock().unwrap();
                inner.$fn_name()
            }
        )*
    };
}

macro_rules! delegate_to_locked_mut_inner {
    ($(($fn_name:ident, $t:ty)),* $(,)?) => {
        $(
            pub fn $fn_name(&self) -> $t {
                let mut inner = self.inner.lock().unwrap();
                inner.$fn_name()
            }
        )*
    };
}

#[derive(Clone, Debug)]
pub struct App {
    inner: Arc<Mutex<AppImpl>>,
}

impl App {
    delegate_to_locked_inner![
        (error_flash_is_empty, bool),
        (feed_ids, Result<Vec<crate::rss::FeedId>>),
        (force_redraw, Result<()>),
        (http_client, ureq::Agent),
        (mode, Mode),
        (selected, Selected),
        (open_link_in_browser, Result<()>),
        (should_quit, bool),
        (refresh_feed, Result<()>),
        (subscribe_to_feed, Result<()>),
        (feed_subscription_input_is_empty, bool),
        (is_renaming, bool)
    ];

    delegate_to_locked_mut_inner![
        (clear_error_flash, ()),
        (clear_flash, ()),
        (on_down, Result<()>),
        (on_left, Result<()>),
        (on_right, Result<()>),
        (on_up, Result<()>),
        (page_up, ()),
        (page_down, ()),
        (pop_feed_subscription_input, ()),
        (put_current_link_in_clipboard, Result<()>),
        (reset_feed_subscription_input, ()),
        (select_feeds, ()),
        (delete_feed, Result<()>),
        (toggle_help, Result<()>),
        (toggle_read, Result<()>),
        (toggle_read_mode, Result<()>),
        (update_current_feed_and_entries, Result<()>),
        (select_and_show_current_entry, Result<()>)
    ];

    pub fn new(
        options: crate::ReadOptions,
        event_tx: std::sync::mpsc::Sender<crate::Event<crossterm::event::KeyEvent>>,
        io_tx: std::sync::mpsc::Sender<crate::io::Action>,
    ) -> Result<App> {
        Ok(App {
            inner: Arc::new(Mutex::new(AppImpl::new(options, event_tx, io_tx)?)),
        })
    }

    pub fn draw(&self, terminal: &mut Terminal<CrosstermBackend<std::io::Stdout>>) -> Result<()> {
        let mut inner = self.inner.lock().unwrap();

        terminal.draw(|f| {
            let chunks = crate::ui::predraw(f);

            assert!(
                chunks.len() >= 2,
                "There must be at least two chunks in order to draw two columns"
            );

            let new_width = chunks[1].width;

            if inner.entry_column_width != new_width {
                inner.entry_column_width = new_width;
                inner.select_and_show_current_entry().unwrap_or_else(|e| {
                    inner.error_flash = vec![e];
                })
            }

            inner.entry_column_width = chunks[1].width;

            crate::ui::draw(f, chunks, &mut inner);
        })?;

        Ok(())
    }

    pub fn set_should_quit(&mut self, should_quit: bool) {
        let mut inner = self.inner.lock().unwrap();
        inner.should_quit = should_quit
    }

    pub fn set_flash(&self, flash: String) {
        let mut inner = self.inner.lock().unwrap();
        inner.flash = Some(flash)
    }

    pub fn push_error_flash(&self, e: anyhow::Error) {
        let mut inner = self.inner.lock().unwrap();
        inner.error_flash.push(e);
    }

    pub fn set_mode(&self, mode: Mode) {
        let mut inner = self.inner.lock().unwrap();
        inner.mode = mode;
    }

    pub fn push_feed_subscription_input(&self, input: char) {
        let mut inner = self.inner.lock().unwrap();
        inner.feed_subscription_input.push(input);
    }

    pub fn set_feeds(&self, feeds: Vec<crate::rss::Feed>) {
        let mut inner = self.inner.lock().unwrap();
        let feeds = feeds.into();
        inner.feeds = feeds;
    }

    pub(crate) fn refresh_feeds(&self) -> Result<()> {
        let feed_ids = self.feed_ids()?;
        let inner = self.inner.lock().unwrap();
        inner
            .io_tx
            .send(crate::io::Action::RefreshFeeds(feed_ids))?;
        Ok(())
    }

    pub(crate) fn break_io_thread(&self) -> Result<()> {
        let inner = self.inner.lock().unwrap();
        inner.io_tx.send(crate::io::Action::Break)?;
        Ok(())
    }

    pub(crate) fn has_entries(&self) -> bool {
        let inner = self.inner.lock().unwrap();
        !inner.entries.items.is_empty()
    }

    pub(crate) fn has_current_entry(&self) -> bool {
        let inner = self.inner.lock().unwrap();
        inner.current_entry_meta.is_some()
    }

    pub(crate) fn cancel_pending_deletion(&self) {
        let mut inner = self.inner.lock().unwrap();
        inner.cancel_pending_deletion();
    }

    pub(crate) fn export_feeds(&self) -> Result<()> {
        let inner = self.inner.lock().unwrap();
        let database_path = inner.database_path.clone();
        drop(inner);

        // generate export path in same directory as database
        let export_dir = database_path.parent()
            .ok_or_else(|| anyhow::anyhow!("database path has no parent directory"))?;
        
        let timestamp = chrono::Utc::now().format("%Y%m%d_%H%M%S");
        let export_path = export_dir.join(format!("rss_tui_export_{}.opml", timestamp));

        let export_options = crate::ExportOptions {
            database_path,
            opml_path: export_path.clone(),
        };

        match crate::opml::export(export_options) {
            Ok(()) => {
                let mut inner = self.inner.lock().unwrap();
                inner.flash = Some(format!("Exported feeds to {:?}", export_path));
                Ok(())
            }
            Err(e) => {
                let mut inner = self.inner.lock().unwrap();
                inner.error_flash.push(e);
                Ok(())
            }
        }
    }

    pub(crate) fn email_article(&self) -> Result<()> {
        let mut inner = self.inner.lock().unwrap();
        inner.email_article()
    }

    pub(crate) fn start_rename_feed(&self) -> Result<()> {
        let mut inner = self.inner.lock().unwrap();
        inner.start_rename_feed()
    }

    pub(crate) fn confirm_rename_feed(&self) -> Result<()> {
        let mut inner = self.inner.lock().unwrap();
        inner.confirm_rename_feed()
    }

    pub(crate) fn cancel_rename_feed(&self) {
        let mut inner = self.inner.lock().unwrap();
        inner.cancel_rename_feed();
    }
}

#[derive(Debug)]
pub struct AppImpl {
    // database stuff
    pub conn: rusqlite::Connection,
    pub database_path: std::path::PathBuf,
    // network stuff
    pub http_client: ureq::Agent,
    // feed stuff
    pub current_feed: Option<crate::rss::Feed>,
    pub feeds: util::StatefulList<crate::rss::Feed>,
    // entry stuff
    pub current_entry_meta: Option<crate::rss::EntryMetadata>,
    pub entries: util::StatefulList<crate::rss::EntryMetadata>,
    pub entry_selection_position: usize,
    pub current_entry_text: String,
    pub entry_scroll_position: u16,
    pub entry_lines_len: usize,
    pub entry_lines_rendered_len: u16,
    pub entry_column_width: u16,
    // modes
    pub should_quit: bool,
    pub selected: Selected,
    pub mode: Mode,
    pub read_mode: ReadMode,
    pub show_help: bool,
    // misc
    pub error_flash: Vec<anyhow::Error>,
    pub feed_subscription_input: String,
    pub flash: Option<String>,
    pub pending_deletion: Option<crate::rss::FeedId>,
    pub pending_rename: Option<crate::rss::FeedId>,
    event_tx: std::sync::mpsc::Sender<crate::Event<crossterm::event::KeyEvent>>,
    io_tx: std::sync::mpsc::Sender<crate::io::Action>,
    pub is_wsl: bool,
}

impl AppImpl {
    pub fn new(
        options: crate::ReadOptions,
        event_tx: std::sync::mpsc::Sender<crate::Event<crossterm::event::KeyEvent>>,
        io_tx: std::sync::mpsc::Sender<crate::io::Action>,
    ) -> Result<AppImpl> {
        let mut conn = rusqlite::Connection::open(&options.database_path)?;

        let http_client = ureq::AgentBuilder::new()
            .timeout_read(options.network_timeout)
            // use crate version as user agent
            .user_agent(&format!("rss-tui/{}", env!("CARGO_PKG_VERSION")))
            .build();

        crate::rss::initialize_db(&mut conn)?;
        let feeds: util::StatefulList<crate::rss::Feed> = vec![].into();
        let entries: util::StatefulList<crate::rss::EntryMetadata> = vec![].into();
        // default to having nothing selected,
        // as it's possible we are starting for the first time,
        // with an empty feeds db
        let selected = Selected::None;
        let initial_current_feed = None;

        let is_wsl = wsl::is_wsl();

        let mut app = AppImpl {
            conn,
            database_path: options.database_path.clone(),
            http_client,
            should_quit: false,
            error_flash: vec![],
            feeds,
            entries,
            selected,
            entry_scroll_position: 0,
            entry_lines_len: 0,
            entry_lines_rendered_len: 0,
            entry_column_width: 0,
            current_entry_meta: None,
            current_entry_text: String::new(),
            current_feed: initial_current_feed,
            feed_subscription_input: String::new(),
            mode: Mode::Normal,
            read_mode: ReadMode::ShowUnread,
            show_help: true,
            entry_selection_position: 0,
            flash: None,
            pending_deletion: None,
            pending_rename: None,
            event_tx,
            is_wsl,
            io_tx,
        };

        app.update_feeds()?;
        app.update_current_feed_and_entries()?;

        // we default to having Selected::None,
        // so if there are actually feeds, select them
        if !app.feeds.items.is_empty() {
            app.select_feeds()
        }

        Ok(app)
    }

    pub fn delete_feed(&mut self) -> Result<()> {
        // handle deletion in editing mode (backward compatibility)
        if matches!(self.selected, Selected::Feeds) && matches!(self.mode(), Mode::Editing) {
            let feed_id = self.selected_feed_id();
            self.perform_feed_deletion(feed_id)?;
            return Ok(());
        }

        // handle deletion in normal mode with confirmation
        if matches!(self.selected, Selected::Feeds) && matches!(self.mode(), Mode::Normal) {
            let feed_id = self.selected_feed_id();
            
            // if already pending deletion for this feed, confirm and delete
            if self.pending_deletion == Some(feed_id) {
                self.perform_feed_deletion(feed_id)?;
                self.pending_deletion = None;
                return Ok(());
            }
            
            // otherwise, set pending deletion and show confirmation message
            self.pending_deletion = Some(feed_id);
            let feed_title = self.feeds.items
                .iter()
                .find(|f| f.id == feed_id)
                .and_then(|f| f.title.as_ref())
                .map(|t| t.as_str())
                .unwrap_or("this feed");
            self.flash = Some(format!("Delete '{}'? Hit 'd' confirm, 'n' to cancel", feed_title));
            return Ok(());
        }

        Ok(())
    }

    fn perform_feed_deletion(&mut self, feed_id: crate::rss::FeedId) -> Result<()> {
        crate::rss::delete_feed(&mut self.conn, feed_id)?;

        // remove the feed in app state
        let feeds_len = self.feeds.items.len();
        let mut feed_title = "Feed".to_string();

        for i in 0..feeds_len {
            if self.feeds.items[i].id == feed_id {
                feed_title = self.feeds.items[i].title.as_ref()
                    .map(|t| t.clone())
                    .unwrap_or_else(|| "Feed".to_string());
                self.feeds.items.remove(i);

                if i == feeds_len - 1 {
                    self.feeds.previous();
                }

                break;
            }
        }

        self.flash = Some(format!("Deleted '{}'", feed_title));

        // remove the entries from the feed in app state
        self.entries.items.retain(|entry| entry.feed_id != feed_id);

        // update
        self.update_current_feed_and_entries()?;
        self.pending_deletion = None;

        Ok(())
    }

    pub fn cancel_pending_deletion(&mut self) {
        self.pending_deletion = None;
        self.flash = None;
    }

    pub fn update_feeds(&mut self) -> Result<()> {
        let feeds = crate::rss::get_feeds(&self.conn)?.into();
        self.feeds = feeds;
        Ok(())
    }

    pub fn update_current_feed_and_entries(&mut self) -> Result<()> {
        self.update_current_feed()?;
        self.update_current_entries()?;
        Ok(())
    }

    fn update_current_feed(&mut self) -> Result<()> {
        self.current_feed = if self.feeds.items.is_empty() {
            self.selected = Selected::None;
            None
        } else {
            let selected_idx = match self.feeds.state.selected() {
                Some(idx) => idx,
                None => {
                    self.feeds.reset();
                    0
                }
            };
            let feed_id = self.feeds.items[selected_idx].id;
            Some(crate::rss::get_feed(&self.conn, feed_id)?)
        };

        Ok(())
    }

    fn update_current_entries(&mut self) -> Result<()> {
        let entries = if let Some(feed) = &self.current_feed {
            crate::rss::get_entries_metas(&self.conn, &self.read_mode, feed.id)?
                .into_iter()
                .collect::<Vec<_>>()
                .into()
        } else {
            vec![].into()
        };

        self.entries = entries;

        if self.entry_selection_position < self.entries.items.len() {
            self.entries
                .state
                .select(Some(self.entry_selection_position))
        } else {
            match self.entries.items.len().checked_sub(1) {
                Some(n) => self.entries.state.select(Some(n)),
                None => self.entries.reset(),
            }
        }
        Ok(())
    }

    fn update_entry_selection_position(&mut self) {
        if self.entries.items.is_empty() {
            self.entry_selection_position = 0
        } else if self.entry_selection_position > self.entries.items.len() - 1 {
            self.entry_selection_position = self.entries.items.len() - 1
        };
    }

    fn get_selected_entry_content(&self) -> Option<Result<crate::rss::EntryContent>> {
        self.entries.state.selected().and_then(|selected_idx| {
            self.entries
                .items
                .get(selected_idx)
                .map(|item| item.id)
                .map(|entry_id| crate::rss::get_entry_content(&self.conn, entry_id))
        })
    }

    fn get_selected_entry_meta(&self) -> Option<Result<crate::rss::EntryMetadata>> {
        self.entries.state.selected().and_then(|selected_idx| {
            self.entries
                .items
                .get(selected_idx)
                .map(|item| item.id)
                .map(|entry_id| crate::rss::get_entry_meta(&self.conn, entry_id))
        })
    }

    fn update_current_entry_meta(&mut self) -> Result<()> {
        if let Some(entry_meta) = self.get_selected_entry_meta() {
            let entry_meta = entry_meta?;
            self.current_entry_meta = Some(entry_meta);
        }
        Ok(())
    }

    fn page_up(&mut self) {
        if matches!(self.selected, Selected::Entry(_)) {
            self.entry_scroll_position = self
                .entry_scroll_position
                .checked_sub(self.entry_lines_rendered_len)
                .unwrap_or_default()
        };
    }

    fn page_down(&mut self) {
        if matches!(self.selected, Selected::Entry(_)) {
            self.entry_scroll_position = if self.entry_scroll_position
                + self.entry_lines_rendered_len
                >= self.entry_lines_len as u16
            {
                self.entry_lines_len as u16
            } else {
                self.entry_scroll_position + self.entry_lines_rendered_len
            };
        }
    }

    pub(crate) fn select_and_show_current_entry(&mut self) -> Result<()> {
        if let Some(entry_meta) = &self.current_entry_meta {
            let entry_meta = entry_meta.clone();

            if let Some(entry) = self.get_selected_entry_content() {
                let entry = entry?;
                let empty_string = String::from("No content or description tag provided.");

                // try content tag first,
                // if there is not content tag,
                // go to description tag,
                // if no description tag,
                // use empty string.
                // TODO figure out what to actually do if there are neither
                let entry_html = entry
                    .content
                    .as_ref()
                    .or(entry.description.as_ref())
                    .or(Some(&empty_string));

                // minimum is 1
                let line_length = if self.entry_column_width >= 5 {
                    self.entry_column_width - 2
                } else {
                    1
                };

                if let Some(html) = entry_html {
                    let text = html2text::from_read(html.as_bytes(), line_length.into())?;
                    self.entry_lines_len = text.matches('\n').count();
                    self.current_entry_text = text;
                } else {
                    self.current_entry_text = String::new();
                }
            }

            self.selected = Selected::Entry(entry_meta);
        }

        Ok(())
    }

    pub(crate) fn refresh_feed(&self) -> Result<()> {
        let feed_id = self.selected_feed_id();
        self.io_tx.send(crate::io::Action::RefreshFeed(feed_id))?;
        Ok(())
    }

    pub(crate) fn subscribe_to_feed(&self) -> Result<()> {
        let feed_subscription_input = self.feed_subscription_input();
        self.io_tx
            .send(crate::io::Action::SubscribeToFeed(feed_subscription_input))?;
        Ok(())
    }

    pub fn start_rename_feed(&mut self) -> Result<()> {
        if matches!(self.selected, Selected::Feeds) && matches!(self.mode(), Mode::Editing) {
            let feed_id = self.selected_feed_id();
            self.pending_rename = Some(feed_id);
            
            // pre-fill input with current title
            let current_title = self.feeds.items
                .iter()
                .find(|f| f.id == feed_id)
                .and_then(|f| f.title.as_ref())
                .map(|t| t.clone())
                .unwrap_or_else(|| String::new());
            
            self.feed_subscription_input = current_title;
            self.flash = None; // clear any existing flash
        }
        Ok(())
    }

    pub fn confirm_rename_feed(&mut self) -> Result<()> {
        if let Some(feed_id) = self.pending_rename {
            let new_title = self.feed_subscription_input.clone();
            
            if new_title.trim().is_empty() {
                self.error_flash.push(anyhow::anyhow!("Feed title cannot be empty"));
                self.pending_rename = None;
                self.reset_feed_subscription_input();
                return Ok(());
            }
            
            crate::rss::update_feed_title(&mut self.conn, feed_id, new_title.trim().to_string())?;
            
            // update the feed in app state
            if let Some(feed) = self.feeds.items.iter_mut().find(|f| f.id == feed_id) {
                feed.title = Some(new_title.trim().to_string());
            }
            
            // update current feed if it's the one being renamed
            if let Some(ref mut current_feed) = self.current_feed {
                if current_feed.id == feed_id {
                    current_feed.title = Some(new_title.trim().to_string());
                }
            }
            
            self.update_feeds()?;
            self.update_current_feed_and_entries()?;
            
            self.flash = Some("Feed renamed".to_string());
            self.pending_rename = None;
            self.reset_feed_subscription_input();
        }
        Ok(())
    }

    pub fn cancel_rename_feed(&mut self) {
        self.pending_rename = None;
        self.reset_feed_subscription_input();
    }

    pub fn toggle_help(&mut self) -> Result<()> {
        self.show_help = !self.show_help;
        Ok(())
    }

    pub fn clear_error_flash(&mut self) {
        self.error_flash = vec![];
    }

    pub fn reset_feed_subscription_input(&mut self) {
        self.feed_subscription_input.clear();
    }

    pub fn pop_feed_subscription_input(&mut self) {
        self.feed_subscription_input.pop();
    }

    pub fn feed_subscription_input_is_empty(&self) -> bool {
        self.feed_subscription_input.is_empty()
    }

    pub fn is_renaming(&self) -> bool {
        self.pending_rename.is_some()
    }

    pub fn feed_subscription_input(&self) -> String {
        self.feed_subscription_input.clone()
    }

    pub fn error_flash_is_empty(&self) -> bool {
        self.error_flash.is_empty()
    }

    pub fn clear_flash(&mut self) {
        self.flash = None
    }

    pub fn select_feeds(&mut self) {
        self.selected = Selected::Feeds;
    }

    pub fn selected(&self) -> Selected {
        self.selected.clone()
    }

    pub fn selected_feed_id(&self) -> crate::rss::FeedId {
        let selected_idx = self.feeds.state.selected().unwrap();
        self.feeds.items[selected_idx].id
    }

    pub fn feed_ids(&self) -> Result<Vec<crate::rss::FeedId>> {
        let ids = crate::rss::get_feed_ids(&self.conn)?;
        Ok(ids)
    }

    pub fn toggle_read(&mut self) -> Result<()> {
        match &self.selected {
            Selected::Entry(entry) => {
                entry.toggle_read(&self.conn)?;
                self.selected = Selected::Entries;
                self.update_current_entries()?;
                self.update_current_entry_meta()?;
                self.entry_scroll_position = 0;
            }
            Selected::Entries => {
                if let Some(entry_meta) = &self.current_entry_meta {
                    entry_meta.toggle_read(&self.conn)?;
                    self.update_current_entries()?;
                    self.update_current_entry_meta()?;
                    self.update_entry_selection_position();
                }
            }
            Selected::Feeds => (),
            Selected::None => (),
        }

        Ok(())
    }

    pub fn http_client(&self) -> ureq::Agent {
        // this is cheap because it only clones a struct containing two Arcs
        self.http_client.clone()
    }

    pub fn toggle_read_mode(&mut self) -> Result<()> {
        self.cancel_pending_deletion();
        self.cancel_rename_feed();
        match (&self.read_mode, &self.selected) {
            (ReadMode::ShowRead, Selected::Feeds) | (ReadMode::ShowRead, Selected::Entries) => {
                self.entry_selection_position = 0;
                self.read_mode = ReadMode::ShowUnread
            }
            (ReadMode::ShowUnread, Selected::Feeds) | (ReadMode::ShowUnread, Selected::Entries) => {
                self.entry_selection_position = 0;
                self.read_mode = ReadMode::ShowRead
            }
            _ => (),
        }
        self.update_current_entries()?;

        if !self.entries.items.is_empty() {
            self.entries.reset();
        } else {
            self.entries.unselect();
        }

        self.update_current_entry_meta()?;

        Ok(())
    }

    fn get_current_link(&self) -> Option<&str> {
        match &self.selected {
            Selected::Feeds => self
                .current_feed
                .as_ref()
                .and_then(|feed| feed.link.as_deref().or(feed.feed_link.as_deref())),
            Selected::Entries => self
                .entries
                .items
                .get(self.entry_selection_position)
                .and_then(|entry| entry.link.as_deref()),
            Selected::Entry(e) => e.link.as_deref(),
            Selected::None => None,
        }
    }

    fn put_current_link_in_clipboard(&mut self) -> Result<()> {
        let current_link = self.get_current_link();

        if self.is_wsl {
            #[cfg(target_os = "linux")]
            {
                if let Some(current_link) = current_link {
                    util::set_wsl_clipboard_contents(current_link)
                } else {
                    Ok(())
                }
            }

            #[cfg(not(target_os = "linux"))]
            {
                unreachable!("This should never happen. This code should only be reachable if the target OS is WSL.")
            }
        } else if let Some(current_link) = current_link {
            let mut ctx = ClipboardContext::new().map_err(|e| anyhow::anyhow!(e))?;
            ctx.set_contents(current_link.to_owned())
                .map_err(|e| anyhow::anyhow!(e))
        } else {
            Ok(())
        }
    }

    fn open_link_in_browser(&self) -> Result<()> {
        if let Some(current_link) = self.get_current_link() {
            webbrowser::open(current_link).map_err(|e| anyhow::anyhow!(e))
        } else {
            Ok(())
        }
    }

    fn email_article(&mut self) -> Result<()> {
        let (title, url) = match &self.selected {
            Selected::Entry(entry_meta) => {
                (
                    entry_meta.title.as_deref().unwrap_or("No title"),
                    entry_meta.link.as_deref(),
                )
            }
            Selected::Entries => {
                if let Some(entry_meta) = &self.current_entry_meta {
                    (
                        entry_meta.title.as_deref().unwrap_or("No title"),
                        entry_meta.link.as_deref(),
                    )
                } else {
                    self.error_flash.push(anyhow::anyhow!("No entry selected"));
                    return Ok(());
                }
            }
            _ => {
                self.error_flash.push(anyhow::anyhow!("No entry selected"));
                return Ok(());
            }
        };

        let url = match url {
            Some(u) => u,
            None => {
                self.error_flash.push(anyhow::anyhow!("Entry has no URL"));
                return Ok(());
            }
        };

        // url-encode title and url for mailto: link
        let encoded_title = urlencoding::encode(title);
        let encoded_url = urlencoding::encode(url);

        // create mailto: URL with subject and body
        let mailto_url = format!("mailto:?subject={}&body={}", encoded_title, encoded_url);

        match webbrowser::open(&mailto_url) {
            Ok(_) => {
                self.flash = Some("Opening email client...".to_string());
            }
            Err(e) => {
                self.error_flash.push(anyhow::anyhow!("Failed to open email client: {}", e));
            }
        }

        Ok(())
    }

    fn should_quit(&self) -> bool {
        self.should_quit
    }

    pub fn on_left(&mut self) -> Result<()> {
        match self.selected {
            Selected::Feeds => (),
            Selected::Entries => {
                self.entry_selection_position = 0;
                self.selected = Selected::Feeds
            }
            Selected::Entry(_) => {
                self.entry_scroll_position = 0;
                self.selected = {
                    self.current_entry_text = String::new();
                    Selected::Entries
                }
            }
            Selected::None => (),
        }

        Ok(())
    }

    pub fn on_up(&mut self) -> Result<()> {
        match self.selected {
            Selected::Feeds => {
                let old_feed_id = if self.pending_deletion.is_some() {
                    Some(self.selected_feed_id())
                } else {
                    None
                };
                self.feeds.previous();
                // cancel pending deletion if we moved to a different feed
                if let (Some(pending_id), Some(old_id)) = (self.pending_deletion, old_feed_id) {
                    let new_feed_id = self.selected_feed_id();
                    if pending_id == old_id && pending_id != new_feed_id {
                        self.cancel_pending_deletion();
                    }
                }
                self.update_current_feed_and_entries()?;
            }
            Selected::Entries => {
                if !self.entries.items.is_empty() {
                    self.entries.previous();
                    self.entry_selection_position = self.entries.state.selected().unwrap();
                    self.update_current_entry_meta()?;
                }
            }
            Selected::Entry(_) => {
                if let Some(n) = self.entry_scroll_position.checked_sub(1) {
                    self.entry_scroll_position = n
                };
            }
            Selected::None => (),
        }

        Ok(())
    }

    pub fn on_right(&mut self) -> Result<()> {
        match self.selected {
            Selected::Feeds => {
                if !self.entries.items.is_empty() {
                    self.cancel_pending_deletion();
                    self.cancel_rename_feed();
                    self.selected = Selected::Entries;
                    self.entries.reset();
                    self.update_current_entry_meta()?;
                }
                Ok(())
            }
            Selected::Entries => self.select_and_show_current_entry(),
            Selected::Entry(_) => Ok(()),
            Selected::None => Ok(()),
        }
    }

    pub fn on_down(&mut self) -> Result<()> {
        match self.selected {
            Selected::Feeds => {
                let old_feed_id = if self.pending_deletion.is_some() {
                    Some(self.selected_feed_id())
                } else {
                    None
                };
                self.feeds.next();
                // cancel pending deletion if we moved to a different feed
                if let (Some(pending_id), Some(old_id)) = (self.pending_deletion, old_feed_id) {
                    let new_feed_id = self.selected_feed_id();
                    if pending_id == old_id && pending_id != new_feed_id {
                        self.cancel_pending_deletion();
                    }
                }
                self.update_current_feed_and_entries()?;
            }
            Selected::Entries => {
                if !self.entries.items.is_empty() {
                    self.entries.next();
                    self.entry_selection_position = self.entries.state.selected().unwrap();
                    self.update_current_entry_meta()?;
                }
            }
            Selected::Entry(_) => {
                if let Some(n) = self.entry_scroll_position.checked_add(1) {
                    self.entry_scroll_position = n
                };
            }
            Selected::None => (),
        }

        Ok(())
    }

    pub fn mode(&self) -> Mode {
        self.mode
    }

    pub fn force_redraw(&self) -> Result<()> {
        self.event_tx.send(crate::Event::Tick).map_err(|e| e.into())
    }
}
