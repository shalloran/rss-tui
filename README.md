# rss-tui

rss-tui [*russ-tooey*] is based on russ, which is a TUI RSS/Atom reader with vim-like controls, written in [Rust](https://rust-lang.org/).

This is a new repo from a personal fork of [ckampfe/russ](https://github.com/ckampfe/russ), with a few improvements.

[![crates.io](https://img.shields.io/crates/v/rss-tui.svg)](https://crates.io/crates/rss-tui)
[![Rust](https://github.com/shalloran/rss-tui/actions/workflows/ci.yml/badge.svg)](https://github.com/shalloran/rss-tui/actions/workflows/ci.yml)

---

![](rss-tui.png)
*rss-tui in all its text-based glory*

## install

### From crates.io (recommended):

```console
cargo install rss-tui
```
Note that on linux, you may need additional system dependencies as well, for example:
```console
sudo apt update && sudo apt install libxcb-shape0-dev libxcb-xfixes0-dev
```
The binary will be installed as `rss-tui`. You can run it with:
```console
rss-tui read
```
**Note:** At this time, macOS, Linux, and Windows (including WSL) are all working. If you run into platform specific issues, please open an issue and specify your OS.

### From this repository:

```console
cargo install rss-tui --git https://github.com/shalloran/rss-tui
```

**Note:** This is a fork with some additional features. If you want the original, use: `cargo install russ --git https://github.com/ckampfe/russ`.

**Note:** If you want to force overwrite an existing installation, use:
`cargo install --force rss-tui` (from crates.io) or `cargo install --force --git https://github.com/shalloran/rss-tui rss-tui` (from git)

**Note** that on its first run with no arguments, `rss-tui read` creates a SQLite database file called `feeds.db` to store RSS/Atom feeds in a location of its choosing. If you wish to override this, you can pass a path with the `-d` option, like `rss-tui -d /your/database/location/my_feeds.db`. If you use a custom database location, you will need to pass the `-d` option every time you invoke `rss-tui`. See the help with `rss-tui -h` for more information about where `rss-tui` will store the `feeds.db` database by default on your platform.

### controls - normal mode

Some normal mode controls vary based on whether you are currently selecting a feed or an entry:

- `q`/`Esc` - quit rss-tui
- `hjkl`/arrows - move up/down/left/right between feeds and entries, scroll up/down on an entry
- `Enter` - read selected entry
- `r` - refresh the selected feed (when feeds selected) or mark entry as read/unread (when entries selected)
- `x` - refresh all feeds
- `i`/`e` - change to insert mode (when feeds selected)
- `e` - email the current article (when viewing an entry; opens your default email client with the article title as subject and URL as body)
- `a` - toggle between read/unread entries
- `c` - copy the selected link to the clipboard (feed or entry)
- `o` - open the selected link in your browser (feed or entry)
- `d` - delete the selected feed (with confirmation; press `d` again to confirm, `n` to cancel)
- `E` - export all feeds to an OPML file (saves to a timestamped file in your database directory)
- `ctrl-u`/`ctrl-d` - scroll up/down a page at a time

### controls - other modes

- There are other modes which will reveal controls to you, but these are helpful:
        - `Esc` - go back to normal mode
        - `Enter` - subscribe to the feed you just typed in the input box
        - `Del` - delete the selected feed

## design

rss-tui is a [tui](https://crates.io/crates/tui) app that uses [crossterm](https://crates.io/crates/crossterm). rss-tui stores all application data in a SQLite database. 

## features/todos

- [x] See [[changelog.md]] for more details...
- [x] [feature]: visual indicator for which feeds have new/unacknowledged entries (partially complete)
- [x] [bug]: text wrapping has been sorted
- [x] [feature]: [issue #44 from ckampfe/russ](https://github.com/ckampfe/russ/issues/44) for a combined feed. I love this idea, so I will implement soon.
- [ ] [feature]: per [issue #39 from ckampfe/russ](https://github.com/ckampfe/russ/issues/39) for a search/filter function, I'll work on this next.
- [ ] [feature]: per [issue #28 from ckampfe/russ](https://github.com/ckampfe/russ/issues/28) for an html text extractor if RSS/ATOM feeds don't show full text
- [ ] [feature]: create a secure github -> crates.io publishing workflow
- [ ] [back-burnered] sync / online mode?
- [ ] [back-burnered] integration with ollama or LMStudio for local summarization pipeline?

## msrv policy

rss-tui targets the latest stable version of the Rust compiler. Older Rust version may work, but this project is not explicitly supporting them.

## SQLite version

`rss-tui` compiles and bundles its own embedded SQLite via the [Rusqlite](https://github.com/rusqlite/rusqlite) project, which is version 3.45.1.

If you prefer to use the version of SQLite on your system, edit `Cargo.toml` to remove the `"bundled"` feature from the `rusqlite` dependency and recompile `rss-tui`.

## contributing

The original project welcomes contributions. If you have an idea for something you would like to contribute to the original, open an issue on [ckampfe/russ](https://github.com/ckampfe/russ) and they can address it. For this fork, I'm happy to consider pull requests, and fix bugs, but keep in mind this is primarily for my own use. If you want a feature that's more broadly useful, please consider contributing to the upstream project as well.

## license

See the [license.](LICENSE)
SPDX-License-Identifier: AGPL-3.0-or-later