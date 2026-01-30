use ratatui::widgets::ListState;

/// strips control chars and zero-width/invisible unicode so TUI rendering isn't broken
pub fn sanitize_for_display(s: &str) -> String {
    s.chars()
        .filter_map(|c| {
            if is_control_or_invisible(c) {
                None
            } else if c == '\n' || c == '\t' || c == '\r' {
                Some(' ')
            } else {
                Some(c)
            }
        })
        .collect()
}

fn is_control_or_invisible(c: char) -> bool {
    if c.is_control() {
        return true;
    }
    matches!(
        c,
        '\u{200B}' | '\u{200C}' | '\u{200D}' | '\u{200E}' | '\u{200F}' | '\u{202A}'..='\u{202E}'
            | '\u{2060}' | '\u{2061}'..='\u{2064}' | '\u{2066}'..='\u{2069}' | '\u{FEFF}'
    )
}

#[derive(Debug)]
pub struct StatefulList<T> {
    pub state: ListState,
    pub items: Vec<T>,
}

impl<T> StatefulList<T> {
    pub fn with_items(items: Vec<T>) -> StatefulList<T> {
        StatefulList {
            state: ListState::default(),
            items,
        }
    }

    pub fn next(&mut self) {
        let i = match self.state.selected() {
            Some(i) => {
                if i >= self.items.len() - 1 {
                    0
                } else {
                    i + 1
                }
            }
            None => 0,
        };
        self.state.select(Some(i));
    }

    pub fn previous(&mut self) {
        let i = match self.state.selected() {
            Some(i) => {
                if i == 0 {
                    self.items.len() - 1
                } else {
                    i - 1
                }
            }
            None => 0,
        };
        self.state.select(Some(i));
    }

    pub fn reset(&mut self) {
        self.state.select(Some(0));
    }

    pub fn unselect(&mut self) {
        self.state.select(None);
    }
}

impl<T> From<Vec<T>> for StatefulList<T> {
    fn from(other: Vec<T>) -> Self {
        StatefulList::with_items(other)
    }
}

// work around for clipboard access in WSL
#[cfg(target_os = "linux")]
pub(crate) fn set_wsl_clipboard_contents(s: &str) -> anyhow::Result<()> {
    use std::{
        io::Write,
        process::{Command, Stdio},
    };

    // it looks like this on the CLI:
    // `echo "foo" | clip.exe`
    let mut clipboard = Command::new("clip.exe").stdin(Stdio::piped()).spawn()?;

    let mut clipboard_stdin = clipboard
        .stdin
        .take()
        .ok_or_else(|| anyhow::anyhow!("Unable to get stdin handle for clip.exe"))?;

    clipboard_stdin.write_all(s.as_bytes())?;

    Ok(())
}
