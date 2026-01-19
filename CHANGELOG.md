# Changelog

We welcome contributions to rss-tui, see README.md for more information.

## 0.6.1

- Add visual indicator for unread entries: feed names now display the count of unread entries in brackets (e.g., "feed-name (5)"). The count only appears when there are unread entries, making it easy to see which feeds have new content at a glance.
- Fix text wrapping for entry titles: entry titles in the entries list now properly wrap to multiple lines instead of being truncated, ensuring long headlines are fully readable.
- Fix flash message display: deletion confirmation prompts and other flash messages now display in a dedicated area above the feeds list instead of being truncated in the feed list title, ensuring important messages are fully visible.
- OPML Import and export is fully functional.
- Deletion of feeds is now possible, confirmation required.
- Added feed rename functionality.

*For all previous versions, see [ckampfe/russ](https://github.com/ckampfe/russ)*
