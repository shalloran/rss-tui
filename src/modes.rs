// here be modes

/// what type of object is currently selected
#[derive(Clone, Debug)]
pub enum Selected {
    Feeds,
    Entries,
    Entry(crate::rss::EntryMetadata),
    /// combined view of all unread entries across feeds ("[feed-name]: title")
    CombinedUnread,
    None,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Mode {
    Editing,
    Normal,
}

#[derive(Clone, Debug)]
pub enum ReadMode {
    ShowRead,
    ShowUnread,
    All,
}
