// retrieving and storing (RSS and Atom) feeds in sqlite db

use crate::modes::ReadMode;
use anyhow::{Context, Result, bail};
use atom_syndication as atom;
use chrono::prelude::{DateTime, Utc};
use html_escape::decode_html_entities_to_string;
use quick_xml::Reader;
use quick_xml::events::Event;
use rss::Channel;
use rusqlite::params;
use rusqlite::types::{FromSql, ToSqlOutput};
use std::collections::HashSet;
use std::fmt::Display;
use std::io::Read;
use std::str::FromStr;

/// entries older than this are pruned on feed refresh to limit db size
const ENTRY_RETENTION_DAYS: u32 = 365;

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct EntryId(i64);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct FeedId(i64);

impl From<i64> for EntryId {
    fn from(value: i64) -> Self {
        Self(value)
    }
}

impl rusqlite::ToSql for EntryId {
    fn to_sql(&self) -> rusqlite::Result<ToSqlOutput<'_>> {
        Ok(self.0.into())
    }
}

impl FromSql for EntryId {
    fn column_result(value: rusqlite::types::ValueRef<'_>) -> rusqlite::types::FromSqlResult<Self> {
        Ok(Self(value.as_i64()?))
    }
}

impl Display for EntryId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<i64> for FeedId {
    fn from(value: i64) -> Self {
        Self(value)
    }
}

impl rusqlite::ToSql for FeedId {
    fn to_sql(&self) -> rusqlite::Result<ToSqlOutput<'_>> {
        Ok(self.0.into())
    }
}

impl FromSql for FeedId {
    fn column_result(value: rusqlite::types::ValueRef<'_>) -> rusqlite::types::FromSqlResult<Self> {
        Ok(Self(value.as_i64()?))
    }
}

impl Display for FeedId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[derive(Clone, Copy, Debug)]
pub enum FeedKind {
    Atom,
    Rss,
}

impl rusqlite::types::FromSql for FeedKind {
    fn column_result(value: rusqlite::types::ValueRef<'_>) -> rusqlite::types::FromSqlResult<Self> {
        let s = value.as_str()?;
        match FeedKind::from_str(s) {
            Ok(feed_kind) => Ok(feed_kind),
            Err(e) => Err(rusqlite::types::FromSqlError::Other(e.into())),
        }
    }
}

impl rusqlite::types::ToSql for FeedKind {
    fn to_sql(&self) -> rusqlite::Result<rusqlite::types::ToSqlOutput<'_>> {
        let s = self.to_string();
        Ok(ToSqlOutput::from(s))
    }
}

impl Display for FeedKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let out = match self {
            FeedKind::Atom => "Atom",
            FeedKind::Rss => "RSS",
        };

        write!(f, "{out}")
    }
}

impl FromStr for FeedKind {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "Atom" => Ok(FeedKind::Atom),
            "RSS" => Ok(FeedKind::Rss),
            _ => Err(anyhow::anyhow!(format!("{s} is not a valid FeedKind"))),
        }
    }
}

/// Feed metadata.
/// Entries are stored separately.
/// The `id` of this type corresponds to `feed_id` on
/// `Entry` and `EntryMeta`.
#[derive(Clone, Debug)]
pub struct Feed {
    pub id: FeedId,
    pub title: Option<String>,
    pub feed_link: Option<String>,
    pub link: Option<String>,
    pub feed_kind: FeedKind,
    pub refreshed_at: Option<chrono::DateTime<Utc>>,
    // these are currently unused:
    // pub inserted_at: chrono::DateTime<Utc>,
    // pub updated_at: chrono::DateTime<Utc>,
    // pub latest_etag: Option<String>,
}

/// This exists:
/// 1. So we can validate an incoming Atom/RSS feed
/// 2. So we can insert it into the database
struct IncomingFeed {
    title: Option<String>,
    feed_link: Option<String>,
    link: Option<String>,
    feed_kind: FeedKind,
    latest_etag: Option<String>,
}

/// This exists:
/// 1. So we can validate an incoming Atom/RSS feed entry
/// 2. So we can insert it into the database
#[derive(Clone)]
struct IncomingEntry {
    title: Option<String>,
    author: Option<String>,
    pub_date: Option<chrono::DateTime<Utc>>,
    description: Option<String>,
    content: Option<String>,
    link: Option<String>,
}

impl From<&atom::Entry> for IncomingEntry {
    fn from(entry: &atom::Entry) -> Self {
        Self {
            title: {
                let mut title = String::new();
                decode_html_entities_to_string(entry.title(), &mut title);
                Some(title)
            },
            author: entry.authors().first().map(|entry_author| {
                let mut author = String::new();
                decode_html_entities_to_string(&entry_author.name, &mut author);
                author
            }),
            pub_date: entry.published().map(|date| date.with_timezone(&Utc)),
            description: None,
            content: entry.content().and_then(|entry_content| {
                entry_content.value().map(|entry_content| {
                    let mut content = String::new();
                    decode_html_entities_to_string(entry_content, &mut content);
                    content
                })
            }),
            link: entry.links().first().map(|link| link.href().to_string()),
        }
    }
}

impl From<&rss::Item> for IncomingEntry {
    fn from(entry: &rss::Item) -> Self {
        Self {
            title: entry.title().map(|entry_title| {
                let mut title = String::new();
                decode_html_entities_to_string(entry_title, &mut title);
                title
            }),
            author: entry.author().map(|entry_author| {
                let mut author = String::new();
                decode_html_entities_to_string(entry_author, &mut author);
                author
            }),
            pub_date: entry.pub_date().and_then(parse_datetime),
            description: entry.description().map(|entry_description| {
                let mut description = String::new();
                decode_html_entities_to_string(entry_description, &mut description);
                description
            }),
            content: entry.content().map(|entry_content| {
                let mut content = String::new();
                decode_html_entities_to_string(entry_content, &mut content);
                content
            }),
            link: entry.link().map(|link| link.to_owned()),
        }
    }
}

/// Metadata for an entry.
///
/// This type exists so we can load entry metadata for lots of
/// entries, without having to load all of the content for those entries,
/// as we only ever need an entry's content in memory when we are displaying
/// the currently selected entry.
#[derive(Clone, Debug)]
pub struct EntryMetadata {
    pub id: EntryId,
    pub feed_id: FeedId,
    pub title: Option<String>,
    // unused:
    // pub author: Option<String>,
    pub pub_date: Option<chrono::DateTime<Utc>>,
    pub link: Option<String>,
    pub read_at: Option<chrono::DateTime<Utc>>,
    pub inserted_at: chrono::DateTime<Utc>,
    // unused:
    // pub updated_at: chrono::DateTime<Utc>,
}

impl EntryMetadata {
    pub fn toggle_read(&self, conn: &rusqlite::Connection) -> Result<()> {
        if self.read_at.is_none() {
            self.mark_as_read(conn)
        } else {
            self.mark_as_unread(conn)
        }
    }

    fn mark_as_read(&self, conn: &rusqlite::Connection) -> Result<()> {
        let mut statement = conn.prepare("UPDATE entries SET read_at = ?2 WHERE id = ?1")?;
        statement.execute(params![self.id, Utc::now()])?;
        Ok(())
    }

    fn mark_as_unread(&self, conn: &rusqlite::Connection) -> Result<()> {
        let mut statement = conn.prepare("UPDATE entries SET read_at = NULL WHERE id = ?1")?;
        statement.execute([self.id])?;
        Ok(())
    }
}

pub struct EntryContent {
    pub content: Option<String>,
    pub description: Option<String>,
}

fn parse_datetime(s: &str) -> Option<DateTime<Utc>> {
    diligent_date_parser::parse_date(s).map(|dt| dt.with_timezone(&Utc))
}

// tag name without namespace: strip prefix (atom:entry -> entry) or Clark notation ({uri}entry -> entry)
fn local_name(name: &[u8]) -> &[u8] {
    if let Some(i) = name.iter().position(|&b| b == b'}')
        && i + 1 < name.len()
    {
        return &name[i + 1..];
    }
    name.splitn(2, |&b| b == b':').last().unwrap_or(name)
}

// streaming parser for feeds using quick-xml
fn parse_feed_streaming<R: Read>(mut reader: R, url: &str) -> Result<FeedAndEntries> {
    let mut buf = Vec::new();
    reader.read_to_end(&mut buf)?;

    let content = String::from_utf8(buf)
        .map_err(|e| anyhow::anyhow!("feed body is not valid utf-8: {}", e))?;

    let mut xml_reader = Reader::from_str(&content);
    xml_reader.config_mut().trim_text(true);

    let mut feed_type: Option<FeedKind> = None;
    let mut feed_title: Option<String> = None;
    let mut feed_link: Option<String> = None;
    let mut entries = Vec::new();

    let mut buf2 = Vec::new();
    let mut in_item = false;
    let mut in_entry = false;

    // temporary storage for current entry/item
    let mut current_entry = IncomingEntry {
        title: None,
        author: None,
        pub_date: None,
        description: None,
        content: None,
        link: None,
    };
    let mut current_text = String::new();
    let mut current_link_href: Option<String> = None;

    loop {
        match xml_reader.read_event_into(&mut buf2) {
            Ok(Event::Start(e)) => {
                let name = String::from_utf8_lossy(local_name(e.name().as_ref())).to_string();

                // detect feed type
                if feed_type.is_none() {
                    match name.as_str() {
                        "feed" => feed_type = Some(FeedKind::Atom),
                        "rss" | "RDF" => feed_type = Some(FeedKind::Rss),
                        _ => {}
                    }
                }

                match name.as_str() {
                    "item" => {
                        in_item = true;
                        current_entry = IncomingEntry {
                            title: None,
                            author: None,
                            pub_date: None,
                            description: None,
                            content: None,
                            link: None,
                        };
                    }
                    "entry" => {
                        in_entry = true;
                        current_entry = IncomingEntry {
                            title: None,
                            author: None,
                            pub_date: None,
                            description: None,
                            content: None,
                            link: None,
                        };
                    }
                    "link" => {
                        // atom: link@href; rss: link text content. try_get_attribute first, then scan by local name (namespaced attrs)
                        current_link_href = None;
                        if let Ok(Some(attr)) = e.try_get_attribute(b"href") {
                            current_link_href =
                                Some(String::from_utf8_lossy(&attr.value).to_string());
                        }
                        if current_link_href.is_none() {
                            for attr in e.attributes().flatten() {
                                let key = String::from_utf8_lossy(local_name(attr.key.as_ref()))
                                    .to_string();
                                if key == "href" {
                                    current_link_href =
                                        Some(String::from_utf8_lossy(&attr.value).to_string());
                                    break;
                                }
                            }
                        }
                        current_text.clear();
                    }
                    "title" | "description" | "content" | "summary" | "author" | "name"
                    | "pubDate" | "published" | "updated" | "dc:date" => {
                        current_text.clear();
                    }
                    _ => {}
                }
            }
            Ok(Event::Empty(e)) => {
                // self-closing tag: treat as Start then End (e.g. <link href="..."/>)
                let name = String::from_utf8_lossy(local_name(e.name().as_ref())).to_string();
                if name == "link" {
                    let mut href = None;
                    if let Ok(Some(attr)) = e.try_get_attribute(b"href") {
                        href = Some(String::from_utf8_lossy(&attr.value).to_string());
                    }
                    if href.is_none() {
                        for attr in e.attributes().flatten() {
                            let key =
                                String::from_utf8_lossy(local_name(attr.key.as_ref())).to_string();
                            if key == "href" {
                                href = Some(String::from_utf8_lossy(&attr.value).to_string());
                                break;
                            }
                        }
                    }
                    if let Some(h) = href {
                        if in_item || in_entry {
                            current_entry.link = Some(h);
                        } else if feed_link.is_none() {
                            feed_link = Some(h);
                        }
                    }
                }
            }
            Ok(Event::Text(e)) => {
                let text = e.unescape().unwrap_or_default();
                current_text.push_str(&text);
            }
            Ok(Event::CData(e)) => {
                let text = String::from_utf8_lossy(&e);
                current_text.push_str(&text);
            }
            Ok(Event::End(e)) => {
                let name = String::from_utf8_lossy(local_name(e.name().as_ref())).to_string();

                match name.as_str() {
                    "item" => {
                        if in_item {
                            entries.push(current_entry.clone());
                            in_item = false;
                        }
                    }
                    "entry" => {
                        if in_entry {
                            entries.push(current_entry.clone());
                            in_entry = false;
                        }
                    }
                    "title" => {
                        if !current_text.is_empty() {
                            let mut decoded = String::new();
                            decode_html_entities_to_string(&current_text, &mut decoded);
                            if in_item || in_entry {
                                current_entry.title = Some(decoded);
                            } else if feed_title.is_none() {
                                feed_title = Some(decoded);
                            }
                        }
                        current_text.clear();
                    }
                    "link" => {
                        if in_item || in_entry {
                            // atom feeds use href attribute, rss feeds use text content
                            if let Some(href) = current_link_href.take() {
                                current_entry.link = Some(href);
                            } else if !current_text.is_empty() {
                                current_entry.link = Some(current_text.clone());
                            }
                        } else if feed_link.is_none() && !current_text.is_empty() {
                            feed_link = Some(current_text.clone());
                        }
                        current_text.clear();
                    }
                    "description" => {
                        if in_item && !current_text.is_empty() {
                            let mut decoded = String::new();
                            decode_html_entities_to_string(&current_text, &mut decoded);
                            current_entry.description = Some(decoded);
                        }
                        current_text.clear();
                    }
                    "content" => {
                        if (in_item || in_entry) && !current_text.is_empty() {
                            let mut decoded = String::new();
                            decode_html_entities_to_string(&current_text, &mut decoded);
                            current_entry.content = Some(decoded);
                        }
                        current_text.clear();
                    }
                    "summary" => {
                        if in_entry && current_entry.content.is_none() && !current_text.is_empty() {
                            let mut decoded = String::new();
                            decode_html_entities_to_string(&current_text, &mut decoded);
                            current_entry.content = Some(decoded);
                        }
                        current_text.clear();
                    }
                    "author" | "name" => {
                        if (in_item || in_entry)
                            && current_entry.author.is_none()
                            && !current_text.is_empty()
                        {
                            let mut decoded = String::new();
                            decode_html_entities_to_string(&current_text, &mut decoded);
                            current_entry.author = Some(decoded);
                        }
                        current_text.clear();
                    }
                    "pubDate" | "published" | "updated" | "dc:date" => {
                        if (in_item || in_entry)
                            && current_entry.pub_date.is_none()
                            && !current_text.is_empty()
                        {
                            current_entry.pub_date = parse_datetime(&current_text);
                        }
                        current_text.clear();
                    }
                    _ => {
                        current_text.clear();
                    }
                }
            }
            Ok(Event::Eof) => break,
            Err(e) => return Err(anyhow::anyhow!("xml parsing error: {}", e)),
            _ => {}
        }
        buf2.clear();
    }

    let feed_kind = feed_type.ok_or_else(|| anyhow::anyhow!("could not determine feed type"))?;

    Ok(FeedAndEntries {
        feed: IncomingFeed {
            title: feed_title,
            feed_link: Some(url.to_string()),
            link: feed_link,
            feed_kind,
            latest_etag: None,
        },
        entries,
    })
}

struct FeedAndEntries {
    pub feed: IncomingFeed,
    pub entries: Vec<IncomingEntry>,
}

impl FeedAndEntries {
    fn set_latest_etag(&mut self, etag: Option<String>) {
        self.feed.latest_etag = etag;
    }
}

impl FromStr for FeedAndEntries {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match atom::Feed::from_str(s) {
            Ok(atom_feed) => {
                let feed = IncomingFeed {
                    title: Some(atom_feed.title.to_string()),
                    feed_link: None,
                    link: atom_feed.links.first().map(|link| link.href().to_string()),
                    feed_kind: FeedKind::Atom,
                    latest_etag: None,
                };

                let entries = atom_feed
                    .entries()
                    .iter()
                    .map(|entry| entry.into())
                    .collect::<Vec<_>>();

                Ok(FeedAndEntries { feed, entries })
            }

            Err(_e) => match Channel::from_str(s) {
                Ok(channel) => {
                    let feed = IncomingFeed {
                        title: Some(channel.title().to_string()),
                        feed_link: None,
                        link: Some(channel.link().to_string()),
                        feed_kind: FeedKind::Rss,
                        latest_etag: None,
                    };

                    let entries = channel
                        .items()
                        .iter()
                        .map(|item| item.into())
                        .collect::<Vec<_>>();

                    Ok(FeedAndEntries { feed, entries })
                }
                Err(e) => Err(e.into()),
            },
        }
    }
}

pub fn validate_and_normalize_feed_url(raw: &str) -> Result<String> {
    let trimmed = raw.trim();

    if trimmed.is_empty() {
        bail!("feed url cannot be empty");
    }

    // if no scheme, assume https
    let candidate = if !trimmed.contains("://") {
        format!("https://{}", trimmed)
    } else {
        trimmed.to_string()
    };

    let url =
        url::Url::parse(&candidate).map_err(|e| anyhow::anyhow!("invalid feed url: {}", e))?;

    match url.scheme() {
        "http" | "https" => Ok(url.to_string()),
        other => {
            bail!(
                "unsupported url scheme '{}', only http and https are allowed",
                other
            );
        }
    }
}

pub fn subscribe_to_feed(
    http_client: &ureq::Agent,
    conn: &mut rusqlite::Connection,
    url: &str,
) -> Result<FeedId> {
    let feed_and_entries = fetch_feed(http_client, url, None)?;

    match feed_and_entries {
        FeedResponse::CacheMiss(feed_and_entries) => {
            let feed_id = in_transaction(conn, |tx| {
                let feed_id = create_feed(tx, &feed_and_entries.feed).with_context(|| {
                    format!(
                        "creating feed {:?} failed",
                        &feed_and_entries.feed.feed_link
                    )
                })?;
                add_entries_to_feed(tx, feed_id, &feed_and_entries.entries).with_context(|| {
                    format!(
                        "inserting {} entries for feed {:?} failed",
                        &feed_and_entries.entries.len(),
                        &feed_and_entries.feed.feed_link
                    )
                })?;
                Ok(feed_id)
            })?;

            Ok(feed_id)
        }
        FeedResponse::CacheHit => {
            bail!("Did not expect feed to be cached in this instance as we did not pass an etag")
        }
    }
}

enum FeedResponse {
    /// The remote host returned a new feed.
    /// The data may not actually be new, as hosts
    /// seem to change etags for all kinds of reasons
    CacheMiss(FeedAndEntries),
    /// the remote host indicated a cache hit,
    /// and did not return any new data
    CacheHit,
}

fn http_status_error_message(status: u16, url: &str) -> String {
    match status {
        400 => format!(
            "bad request (400) fetching feed {}. the server rejected the request - check the url",
            url
        ),
        401 => format!(
            "unauthorized (401) fetching feed {}. authentication may be required",
            url
        ),
        403 => format!(
            "forbidden (403) fetching feed {}. access denied - the server refused the request",
            url
        ),
        404 => format!(
            "not found (404) fetching feed {}. the feed url may be incorrect or the feed may have been removed",
            url
        ),
        408 => format!(
            "request timeout (408) fetching feed {}. the server took too long to respond",
            url
        ),
        429 => format!(
            "too many requests (429) fetching feed {}. rate limited - wait a moment and try again",
            url
        ),
        500..=599 => format!(
            "server error ({}) fetching feed {}. this could be temporary - check the site in a browser and try again later",
            status, url
        ),
        300..=399 => format!(
            "redirect error ({}). the server returned an unexpected redirect for feed {}",
            status, url
        ),
        _ => format!("unexpected status code {} fetching feed {}", status, url),
    }
}

fn fetch_feed(
    http_client: &ureq::Agent,
    url: &str,
    current_etag: Option<String>,
) -> Result<FeedResponse> {
    let request = http_client.get(url);

    let request = if let Some(etag) = current_etag {
        request.set("If-None-Match", &etag)
    } else {
        request
    };

    let response = request.call().with_context(|| {
        format!(
            "network error fetching feed {}. check your internet connection and verify the url is accessible",
            url
        )
    })?;

    let status = response.status();

    match status {
        // the etags did not match, it is a new feed file
        200 => {
            let header_names = response.headers_names();

            let etag_header_name = header_names
                .iter()
                .find(|header_name| header_name.to_lowercase() == "etag");

            let etag = etag_header_name
                .and_then(|etag_header| response.header(etag_header))
                .map(|etag| etag.to_owned());

            let reader = response.into_reader();

            let mut feed_and_entries = parse_feed_streaming(reader, url).with_context(|| {
                format!(
                    "failed to parse feed from {}. the response is not valid rss or atom xml",
                    url
                )
            })?;

            feed_and_entries.set_latest_etag(etag);

            Ok(FeedResponse::CacheMiss(feed_and_entries))
        }
        // the etags match, it is the same feed we already have
        304 => Ok(FeedResponse::CacheHit),
        status => Err(anyhow::anyhow!(
            "{}",
            http_status_error_message(status, url)
        )),
    }
}

fn prune_old_entries_for_feed(
    tx: &rusqlite::Transaction,
    feed_id: FeedId,
    max_age_days: u32,
) -> Result<()> {
    let cutoff = Utc::now() - chrono::Duration::days(max_age_days as i64);
    tx.execute(
        "DELETE FROM entries WHERE feed_id = ?1 AND COALESCE(pub_date, inserted_at) < ?2",
        params![feed_id, cutoff],
    )?;
    Ok(())
}

/// fetches the feed and stores the new entries
/// uses the link as the uniqueness key.
/// TODO hash the content to see if anything changed, and update that way.
pub fn refresh_feed(
    client: &ureq::Agent,
    conn: &mut rusqlite::Connection,
    feed_id: FeedId,
) -> Result<()> {
    let feed_url = get_feed_url(conn, feed_id)
        .with_context(|| format!("Unable to get url for feed id {feed_id} from the database",))?;

    let current_etag = get_feed_latest_etag(conn, feed_id).with_context(|| {
        format!("Unable to get latest_etag for feed_id {feed_id} from the database")
    })?;

    let remote_feed = fetch_feed(client, &feed_url, current_etag)
        .with_context(|| format!("Failed to fetch feed {feed_url}"))?;

    if let FeedResponse::CacheMiss(remote_feed) = remote_feed {
        let remote_items = remote_feed.entries;
        let remote_items_links = remote_items
            .iter()
            .flat_map(|item| &item.link)
            .cloned()
            .collect::<HashSet<String>>();

        let local_entries_links = get_entries_links(conn, &ReadMode::All, feed_id)?
            .into_iter()
            .flatten()
            .collect::<HashSet<_>>();

        let difference = remote_items_links
            .difference(&local_entries_links)
            .cloned()
            .collect::<HashSet<_>>();

        let items_to_add = remote_items
            .into_iter()
            .filter(|item| match &item.link {
                Some(link) => difference.contains(link.as_str()),
                None => false,
            })
            .collect::<Vec<_>>();

        in_transaction(conn, |tx| {
            add_entries_to_feed(tx, feed_id, &items_to_add)?;
            update_feed_refreshed_at(tx, feed_id)?;
            update_feed_etag(tx, feed_id, remote_feed.feed.latest_etag.clone())?;
            prune_old_entries_for_feed(tx, feed_id, ENTRY_RETENTION_DAYS)?;
            Ok(())
        })?;
    } else {
        in_transaction(conn, |tx| {
            update_feed_refreshed_at(tx, feed_id)?;
            prune_old_entries_for_feed(tx, feed_id, ENTRY_RETENTION_DAYS)?;
            Ok(())
        })?;
    }

    Ok(())
}

pub fn initialize_db(conn: &mut rusqlite::Connection) -> Result<()> {
    in_transaction(conn, |tx| {
        let schema_version: u64 = tx.pragma_query_value(None, "user_version", |row| row.get(0))?;

        if schema_version == 0 {
            tx.pragma_update(None, "user_version", 1)?;

            tx.execute(
                "CREATE TABLE IF NOT EXISTS feeds (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        title TEXT,
        feed_link TEXT,
        link TEXT,
        feed_kind TEXT,
        refreshed_at TIMESTAMP,
        inserted_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
        updated_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
        )",
                [],
            )?;

            tx.execute(
                "CREATE TABLE IF NOT EXISTS entries (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        feed_id INTEGER,
        title TEXT,
        author TEXT,
        pub_date TIMESTAMP,
        description TEXT,
        content TEXT,
        link TEXT,
        read_at TIMESTAMP,
        inserted_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
        updated_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
        )",
                [],
            )?;

            tx.execute(
                "CREATE INDEX IF NOT EXISTS entries_feed_id_and_pub_date_and_inserted_at_index
        ON entries (feed_id, pub_date, inserted_at)",
                [],
            )?;
        }

        if schema_version <= 1 {
            tx.pragma_update(None, "user_version", 2)?;

            tx.execute("ALTER TABLE feeds ADD COLUMN latest_etag TEXT", [])?;
        }

        if schema_version <= 2 {
            tx.pragma_update(None, "user_version", 3)?;

            tx.execute(
                "CREATE UNIQUE INDEX IF NOT EXISTS feeds_feed_link ON feeds (feed_link)",
                [],
            )?;
        }

        Ok(())
    })
}

fn create_feed(tx: &rusqlite::Transaction, feed: &IncomingFeed) -> Result<FeedId> {
    let feed_id = tx.query_row::<FeedId, _, _>(
        "INSERT INTO feeds (title, link, feed_link, feed_kind)
        VALUES (?1, ?2, ?3, ?4)
        RETURNING id",
        params![feed.title, feed.link, feed.feed_link, feed.feed_kind],
        |r| r.get(0),
    )?;

    Ok(feed_id)
}

pub fn delete_feed(conn: &mut rusqlite::Connection, feed_id: FeedId) -> Result<()> {
    in_transaction(conn, |tx| {
        tx.execute("DELETE FROM feeds WHERE id = ?1", [feed_id])?;
        tx.execute("DELETE FROM entries WHERE feed_id = ?1", [feed_id])?;
        Ok(())
    })
}

fn add_entries_to_feed(
    tx: &rusqlite::Transaction,
    feed_id: FeedId,
    entries: &[IncomingEntry],
) -> Result<()> {
    if !entries.is_empty() {
        let now = Utc::now();

        let mut insert_statement = tx.prepare(
            "INSERT INTO entries (feed_id, title, author, pub_date, description, content, link, updated_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
        )?;

        // in most databases, doing this kind of "multiple inserts in a loop" thing would be bad and slow, but it's ok here because:
        // 1. it is within single a transaction. in SQLite, doing many writes in the same transaction is actually fast
        // 2. it is with single prepared statement, which further improves its write throughput
        // see further: https://stackoverflow.com/questions/1711631/improve-insert-per-second-performance-of-sqlite
        for entry in entries {
            insert_statement.execute(params![
                feed_id,
                entry.title,
                entry.author,
                entry.pub_date,
                entry.description,
                entry.content,
                entry.link,
                now
            ])?;
        }
    }

    Ok(())
}

pub fn get_feed(conn: &rusqlite::Connection, feed_id: FeedId) -> Result<Feed> {
    let s = conn.query_row(
        "SELECT id, title, feed_link, link, feed_kind, refreshed_at, inserted_at, updated_at, latest_etag FROM feeds WHERE id=?1",
        [feed_id],
        |row| {
            let feed_kind_str: String = row.get(4)?;
            let feed_kind: FeedKind = FeedKind::from_str(&feed_kind_str)
                .unwrap_or_else(|_| panic!("FeedKind must be Atom or RSS, got {feed_kind_str}"));

            Ok(Feed {
                id: row.get(0)?,
                title: row.get(1)?,
                feed_link: row.get(2)?,
                link: row.get(3)?,
                feed_kind,
                refreshed_at: row.get(5)?,
                // inserted_at: row.get(6)?,
                // updated_at: row.get(7)?,
                // latest_etag: row.get(8)?,
            })
        },
    )?;

    Ok(s)
}

fn update_feed_refreshed_at(tx: &rusqlite::Transaction, feed_id: FeedId) -> Result<()> {
    tx.execute(
        "UPDATE feeds SET refreshed_at = ?2 WHERE id = ?1",
        params![feed_id, Utc::now()],
    )?;

    Ok(())
}

fn update_feed_etag(
    tx: &rusqlite::Transaction,
    feed_id: FeedId,
    latest_etag: Option<String>,
) -> Result<()> {
    tx.execute(
        "UPDATE feeds SET latest_etag = ?2 WHERE id = ?1",
        params![feed_id, latest_etag],
    )?;

    Ok(())
}

pub fn update_feed_title(
    conn: &mut rusqlite::Connection,
    feed_id: FeedId,
    new_title: String,
) -> Result<()> {
    in_transaction(conn, |tx| {
        tx.execute(
            "UPDATE feeds SET title = ?2 WHERE id = ?1",
            params![feed_id, new_title],
        )?;
        Ok(())
    })
}

pub fn get_feed_url(conn: &rusqlite::Connection, feed_id: FeedId) -> Result<String> {
    let s: String = conn.query_row(
        "SELECT feed_link FROM feeds WHERE id=?1",
        [feed_id],
        |row| row.get(0),
    )?;

    Ok(s)
}

fn get_feed_latest_etag(conn: &rusqlite::Connection, feed_id: FeedId) -> Result<Option<String>> {
    let s: Option<String> = conn.query_row(
        "SELECT latest_etag FROM feeds WHERE id=?1",
        [feed_id],
        |row| {
            let etag: Option<String> = row.get(0)?;
            Ok(etag)
        },
    )?;

    Ok(s)
}

pub fn get_feeds(conn: &rusqlite::Connection) -> Result<Vec<Feed>> {
    let mut statement = conn.prepare(
        "SELECT 
          id, 
          title, 
          feed_link, 
          link, 
          feed_kind, 
          refreshed_at
          -- inserted_at,
          -- updated_at,
          -- latest_etag
        FROM feeds ORDER BY lower(title) ASC",
    )?;
    let mut feeds = vec![];
    for feed in statement.query_map([], |row| {
        Ok(Feed {
            id: row.get(0)?,
            title: row.get(1)?,
            feed_link: row.get(2)?,
            link: row.get(3)?,
            feed_kind: row.get(4)?,
            refreshed_at: row.get(5)?,
            // inserted_at: row.get(6)?,
            // updated_at: row.get(7)?,
            // latest_etag: row.get(8)?,
        })
    })? {
        feeds.push(feed?)
    }

    Ok(feeds)
}

pub fn get_feed_ids(conn: &rusqlite::Connection) -> Result<Vec<FeedId>> {
    let mut statement = conn.prepare("SELECT id FROM feeds ORDER BY lower(title) ASC")?;
    let mut ids = vec![];
    for id in statement.query_map([], |row| row.get(0))? {
        ids.push(id?)
    }

    Ok(ids)
}

// count unread entries for a specific feed
pub fn count_unread_entries(conn: &rusqlite::Connection, feed_id: FeedId) -> Result<usize> {
    let count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM entries WHERE feed_id = ?1 AND read_at IS NULL",
        [feed_id],
        |row| row.get(0),
    )?;
    Ok(count as usize)
}

/// Returns entry counts per day for the last N days for sparkline display.
/// Returns a vector of counts, one per day, from oldest to newest.
pub fn get_feed_activity(
    conn: &rusqlite::Connection,
    feed_id: FeedId,
    days: u32,
) -> Result<Vec<u64>> {
    let start_date = Utc::now() - chrono::Duration::days(days as i64);

    // Query entries grouped by date
    let mut statement = conn.prepare(
        "SELECT DATE(COALESCE(pub_date, inserted_at)) as day, COUNT(*) as count
         FROM entries
         WHERE feed_id = ?1
         AND COALESCE(pub_date, inserted_at) >= ?2
         GROUP BY day
         ORDER BY day ASC",
    )?;

    let mut day_counts: std::collections::HashMap<String, u64> = std::collections::HashMap::new();

    for row in statement.query_map(params![feed_id, start_date], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, u64>(1)?))
    })? {
        let (day, count) = row?;
        day_counts.insert(day, count);
    }

    // Fill in missing days with 0, from oldest to newest
    let mut activity = Vec::with_capacity(days as usize);
    for i in (0..days).rev() {
        let date = (Utc::now() - chrono::Duration::days(i as i64))
            .format("%Y-%m-%d")
            .to_string();
        activity.push(*day_counts.get(&date).unwrap_or(&0));
    }

    Ok(activity)
}

pub fn get_entry_meta(conn: &rusqlite::Connection, entry_id: EntryId) -> Result<EntryMetadata> {
    let result = conn.query_row(
        "SELECT 
          id,
          feed_id,
          title,
          -- author,
          pub_date,
          link,
          read_at,
          inserted_at
          -- updated_at
        FROM entries WHERE id=?1",
        [entry_id],
        |row| {
            Ok(EntryMetadata {
                id: row.get(0)?,
                feed_id: row.get(1)?,
                title: row.get(2)?,
                // author: row.get(3)?,
                pub_date: row.get(3)?,
                link: row.get(4)?,
                read_at: row.get(5)?,
                inserted_at: row.get(6)?,
                // updated_at: row.get(8)?,
            })
        },
    )?;

    Ok(result)
}

pub fn get_entry_content(conn: &rusqlite::Connection, entry_id: EntryId) -> Result<EntryContent> {
    let result = conn.query_row(
        "SELECT content, description FROM entries WHERE id=?1",
        [entry_id],
        |row| {
            Ok(EntryContent {
                content: row.get(0)?,
                description: row.get(1)?,
            })
        },
    )?;

    Ok(result)
}

pub fn get_entries_metas(
    conn: &rusqlite::Connection,
    read_mode: &ReadMode,
    feed_id: FeedId,
) -> Result<Vec<EntryMetadata>> {
    let read_at_predicate = match read_mode {
        ReadMode::ShowUnread => "\nAND read_at IS NULL",
        ReadMode::ShowRead => "\nAND read_at IS NOT NULL",
        ReadMode::All => "\n",
    };

    // we get weird pubDate formats from feeds,
    // so sort by inserted at as this as a stable order at least
    let mut query = "SELECT 
        id,
        feed_id,
        title,
        -- author,
        pub_date,
        link,
        read_at,
        inserted_at
        -- updated_at
        FROM entries 
        WHERE feed_id=?1"
        .to_string();

    query.push_str(read_at_predicate);
    query.push_str("\nORDER BY pub_date DESC, inserted_at DESC");

    let mut statement = conn.prepare(&query)?;
    let mut entries = vec![];
    for entry in statement.query_map([feed_id], |row| {
        Ok(EntryMetadata {
            id: row.get(0)?,
            feed_id: row.get(1)?,
            title: row.get(2)?,
            // unused:
            // author: row.get(3)?,
            pub_date: row.get(3)?,
            link: row.get(4)?,
            read_at: row.get(5)?,
            inserted_at: row.get(6)?,
            // unused:
            // updated_at: row.get(8)?,
        })
    })? {
        entries.push(entry?)
    }

    Ok(entries)
}

/// all unread entries across feeds, each paired with its feed title for combined view
pub fn get_all_unread_entries_with_feed_name(
    conn: &rusqlite::Connection,
) -> Result<Vec<(String, EntryMetadata)>> {
    let mut statement = conn.prepare(
        "SELECT e.id, e.feed_id, e.title, e.pub_date, e.link, e.read_at, e.inserted_at, f.title AS feed_title
         FROM entries e
         JOIN feeds f ON e.feed_id = f.id
         WHERE e.read_at IS NULL
         ORDER BY e.pub_date DESC, e.inserted_at DESC",
    )?;
    let mut out = vec![];
    for row in statement.query_map([], |row| {
        let entry = EntryMetadata {
            id: row.get(0)?,
            feed_id: row.get(1)?,
            title: row.get(2)?,
            pub_date: row.get(3)?,
            link: row.get(4)?,
            read_at: row.get(5)?,
            inserted_at: row.get(6)?,
        };
        let feed_title: Option<String> = row.get(7)?;
        Ok((feed_title.unwrap_or_else(|| "?".to_string()), entry))
    })? {
        out.push(row?);
    }
    Ok(out)
}

pub fn get_entries_links(
    conn: &rusqlite::Connection,
    read_mode: &ReadMode,
    feed_id: FeedId,
) -> Result<Vec<Option<String>>> {
    let read_at_predicate = match read_mode {
        ReadMode::ShowUnread => "\nAND read_at IS NULL",
        ReadMode::ShowRead => "\nAND read_at IS NOT NULL",
        ReadMode::All => "\n",
    };

    // we get weird pubDate formats from feeds,
    // so sort by inserted at as this as a stable order at least
    let mut query = "SELECT link FROM entries WHERE feed_id=?1".to_string();

    query.push_str(read_at_predicate);
    query.push_str("\nORDER BY pub_date DESC, inserted_at DESC");

    let mut links = vec![];
    let mut statement = conn.prepare(&query)?;

    for link in statement.query_map([feed_id], |row| row.get(0))? {
        links.push(link?);
    }

    Ok(links)
}

/// run `f` in a transaction, committing if `f` returns an `Ok` value,
/// otherwise rolling back.
fn in_transaction<F, R>(conn: &mut rusqlite::Connection, f: F) -> Result<R>
where
    F: Fn(&rusqlite::Transaction) -> Result<R>,
{
    let tx = conn.transaction()?;

    let result = f(&tx)?;

    tx.commit()?;

    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    const ZCT: &str = "https://zeroclarkthirty.com/feed";

    #[test]
    fn atom_feed_with_default_namespace_parses() {
        // atom with default namespace (typical real-world atom)
        let atom = r#"<?xml version="1.0"?>
<feed xmlns="http://www.w3.org/2005/Atom">
  <title>Test Feed</title>
  <link href="http://example.com/"/>
  <entry>
    <title>Entry 1</title>
    <link href="http://example.com/1"/>
  </entry>
</feed>"#;
        let result = parse_feed_streaming(atom.as_bytes(), "http://example.com/feed");
        let fa = result.expect("parse should succeed");
        assert!(matches!(fa.feed.feed_kind, FeedKind::Atom));
        assert_eq!(fa.entries.len(), 1, "expected one entry");
        assert_eq!(
            fa.entries[0].title.as_deref(),
            Some("Entry 1"),
            "entry title"
        );
        assert_eq!(
            fa.entries[0].link.as_deref(),
            Some("http://example.com/1"),
            "entry link (atom uses link@href)"
        );
    }

    #[test]
    fn atom_feed_no_namespace_parses() {
        // atom with no namespace (bare tags) – must still work
        let atom = r#"<?xml version="1.0"?>
<feed>
  <title>Test Feed</title>
  <link href="http://example.com/"/>
  <entry>
    <title>Entry 1</title>
    <link href="http://example.com/1"/>
  </entry>
</feed>"#;
        let result = parse_feed_streaming(atom.as_bytes(), "http://example.com/feed");
        let fa = result.expect("parse should succeed");
        assert!(matches!(fa.feed.feed_kind, FeedKind::Atom));
        assert_eq!(fa.entries.len(), 1);
        assert_eq!(fa.entries[0].link.as_deref(), Some("http://example.com/1"));
    }

    #[test]
    fn it_fetches() {
        let http_client = ureq::AgentBuilder::new()
            .timeout_read(std::time::Duration::from_secs(5))
            .build();
        let feed_and_entries = fetch_feed(&http_client, ZCT, None).unwrap();
        if let FeedResponse::CacheMiss(feed_and_entries) = feed_and_entries {
            assert!(!feed_and_entries.entries.is_empty())
        } else {
            panic!("somehow got a cached response when passing no etag")
        }
    }

    #[test]
    fn it_subscribes_to_a_feed() {
        let http_client = ureq::AgentBuilder::new()
            .timeout_read(std::time::Duration::from_secs(5))
            .build();
        let mut conn = rusqlite::Connection::open_in_memory().unwrap();
        initialize_db(&mut conn).unwrap();
        subscribe_to_feed(&http_client, &mut conn, ZCT).unwrap();
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM entries", [], |row| row.get(0))
            .unwrap();

        assert!(count > 50)
    }

    #[test]
    fn validate_and_normalize_feed_url_works_for_https() {
        let url = validate_and_normalize_feed_url("https://example.com/feed").unwrap();
        assert_eq!(url, "https://example.com/feed");
    }

    #[test]
    fn validate_and_normalize_feed_url_adds_https_when_missing() {
        let url = validate_and_normalize_feed_url("example.com/feed").unwrap();
        assert!(url.starts_with("https://"));
    }

    #[test]
    fn validate_and_normalize_feed_url_rejects_empty() {
        let err = validate_and_normalize_feed_url("  ").unwrap_err();
        assert!(
            err.to_string()
                .to_lowercase()
                .contains("feed url cannot be empty")
        );
    }

    #[test]
    fn refresh_feed_does_not_add_any_items_if_there_are_no_new_items() {
        let http_client = ureq::AgentBuilder::new()
            .timeout_read(std::time::Duration::from_secs(5))
            .build();
        let mut conn = rusqlite::Connection::open_in_memory().unwrap();
        initialize_db(&mut conn).unwrap();
        subscribe_to_feed(&http_client, &mut conn, ZCT).unwrap();
        let feed_id = 1.into();
        let old_unread = get_entries_metas(&conn, &ReadMode::ShowUnread, feed_id).unwrap();
        refresh_feed(&http_client, &mut conn, feed_id).unwrap();
        let after_refresh_unread =
            get_entries_metas(&conn, &ReadMode::ShowUnread, feed_id).unwrap();
        // refresh never adds when remote unchanged; count may drop due to retention prune
        assert!(
            after_refresh_unread.len() <= old_unread.len(),
            "refresh must not add items"
        );
        let e = get_entry_meta(&conn, 1.into()).unwrap();
        e.mark_as_read(&conn).unwrap();
        let new_unread = get_entries_metas(&conn, &ReadMode::ShowUnread, feed_id).unwrap();
        assert_eq!(new_unread.len(), after_refresh_unread.len() - 1);
    }

    #[test]
    fn works_transactionally() {
        let mut conn = rusqlite::Connection::open_in_memory().unwrap();

        conn.execute("CREATE TABLE foo (t)", []).unwrap();

        let count: i64 = conn
            .query_row("select count(*) from foo", [], |row| row.get(0))
            .unwrap();

        // should be nothing in the table
        assert_eq!(count, 0);

        // insert one row to prove it works
        let _ = in_transaction(&mut conn, |tx| {
            tx.execute(r#"INSERT INTO foo (t) values ("some initial string")"#, [])?;
            Ok(())
        });

        let count: i64 = conn
            .query_row("select count(*) from foo", [], |row| row.get(0))
            .unwrap();

        // we inserted one row, there should be one
        assert_eq!(count, 1);

        // do 2 inserts in the same way as before, but error in the middle of the inserts.
        // this should rollback
        let tr = in_transaction(&mut conn, |tx| {
            tx.execute(r#"INSERT INTO foo (t) values ("some string")"#, [])?;
            tx.execute("this is not valid sql, it should error and rollback", [])?;
            tx.execute(r#"INSERT INTO foo (t) values ("some other string")"#, [])?;

            Ok(())
        });

        // it should be an error
        let e = tr.unwrap_err();
        assert!(e.to_string().contains("syntax error"));

        let count: i64 = conn
            .query_row("select count(*) from foo", [], |row| row.get(0))
            .unwrap();

        // assert that no further entries have been inserted
        assert_eq!(count, 1);
    }
}
