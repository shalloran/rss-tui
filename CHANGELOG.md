# Changelog

We welcome contributions to rss-tui, see README.md for more information.

## 0.6.6
commit message: handle large feeds and improve error/reporting ux, fixes #1

- stream feed bodies with a 4mb cap instead of using ureq into_string
- add url validation/normalization before subscribing to feeds
- surface clearer network/http/parse errors with actionable guidance
- show theme toggle helper for `t` in all help views
- keep formatting and clippy clean to match ci expectations
- **Note:** Toggle themes with `t` if you don't like the new default, and also to prevent the sqlite db from getting too large, I've capped the history for each feed at 365 days.

## 0.6.2, 0.6.3

- Bumped versions for minor edits, github actions, format, clippy.

## 0.6.1

- Add visual indicator for unread entries: feed names now display the count of unread entries in brackets (e.g., "feed-name (5)"). The count only appears when there are unread entries, making it easy to see which feeds have new content at a glance.
- Fix text wrapping for entry titles: entry titles in the entries list now properly wrap to multiple lines instead of being truncated, ensuring long headlines are fully readable.
- Fix flash message display: deletion confirmation prompts and other flash messages now display in a dedicated area above the feeds list instead of being truncated in the feed list title, ensuring important messages are fully visible.
- OPML Import and export is fully functional.
- Deletion of feeds is now possible, confirmation required.
- Added feed rename functionality.

*For all previous versions, see [ckampfe/russ](https://github.com/ckampfe/russ)*
