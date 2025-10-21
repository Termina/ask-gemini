use anstyle::{AnsiColor, Style};
use std::io::{self, Write};

/// Tracks how the current line should be rendered.
#[derive(Copy, Clone, Eq, PartialEq)]
enum LineState {
  Unknown,
  Plain,
  Heading,
  Bullet,
  Quote,
}

/// Incrementally applies lightweight styling to streamed text without buffering it fully.
pub struct StreamingFormatter<W: Write> {
  writer: W,
  in_code_block: bool,
  pending_backticks: usize,
  pending_asterisks: usize,
  line_state: LineState,
  line_has_only_space: bool,
  last_char_was_newline: bool,
  inline_bold: bool,
  markdown_enabled: bool,
}

impl<W: Write> StreamingFormatter<W> {
  pub fn styled(writer: W) -> Self {
    Self::with_mode(writer, true)
  }

  pub fn plain(writer: W) -> Self {
    Self::with_mode(writer, false)
  }

  fn with_mode(writer: W, markdown_enabled: bool) -> Self {
    Self {
      writer,
      in_code_block: false,
      pending_backticks: 0,
      pending_asterisks: 0,
      line_state: LineState::Unknown,
      line_has_only_space: true,
      last_char_was_newline: true,
      inline_bold: false,
      markdown_enabled,
    }
  }

  pub fn write_chunk(&mut self, chunk: &str) -> io::Result<()> {
    for ch in chunk.chars() {
      if ch == '`' {
        self.handle_backtick()?;
        continue;
      }

      if self.markdown_enabled && !self.in_code_block && ch == '*' {
        self.handle_asterisk()?;
        continue;
      } else {
        if self.pending_backticks > 0 {
          self.flush_literal_backticks()?;
        }
        if self.pending_asterisks > 0 {
          self.flush_literal_asterisks()?;
        }
        self.write_char(ch)?;
      }
    }

    self.writer.flush()
  }

  pub fn finish(&mut self) -> io::Result<()> {
    if self.pending_backticks > 0 {
      self.flush_literal_backticks()?;
    }
    if self.pending_asterisks > 0 {
      self.flush_literal_asterisks()?;
    }
    self.inline_bold = false;

    if !self.last_char_was_newline {
      self.writer.write_all(b"\n")?;
      self.last_char_was_newline = true;
    }

    self.writer.flush()
  }

  fn handle_backtick(&mut self) -> io::Result<()> {
    if self.pending_asterisks > 0 {
      self.flush_literal_asterisks()?;
    }
    self.pending_backticks += 1;

    if self.pending_backticks == 3 {
      self.write_fence()?;
      self.in_code_block = !self.in_code_block;
      self.pending_backticks = 0;
      self.line_state = LineState::Unknown;
      self.line_has_only_space = true;
    }

    Ok(())
  }

  fn flush_literal_backticks(&mut self) -> io::Result<()> {
    while self.pending_backticks > 0 {
      self.emit_with_current_style("`")?;
      self.pending_backticks -= 1;
      self.last_char_was_newline = false;
    }
    Ok(())
  }

  fn handle_asterisk(&mut self) -> io::Result<()> {
    self.pending_asterisks += 1;

    if self.pending_asterisks == 2 {
      self.inline_bold = !self.inline_bold;
      self.pending_asterisks = 0;
    }

    Ok(())
  }

  fn flush_literal_asterisks(&mut self) -> io::Result<()> {
    while self.pending_asterisks > 0 {
      self.emit_with_current_style("*")?;
      self.pending_asterisks -= 1;
      self.last_char_was_newline = false;
    }
    Ok(())
  }

  fn write_char(&mut self, ch: char) -> io::Result<()> {
    if ch == '\r' {
      return Ok(());
    }

    if ch == '\n' {
      if self.pending_asterisks > 0 {
        self.flush_literal_asterisks()?;
      }
      self.writer.write_all(b"\n")?;
      self.line_state = LineState::Unknown;
      self.line_has_only_space = true;
      self.last_char_was_newline = true;
      self.inline_bold = false;
      return Ok(());
    }

    if self.markdown_enabled && !self.in_code_block {
      match self.line_state {
        LineState::Unknown => match ch {
          '#' if self.line_has_only_space => {
            self.line_state = LineState::Heading;
            self.line_has_only_space = false;
          }
          '-' | '*' if self.line_has_only_space => {
            self.line_state = LineState::Bullet;
            self.line_has_only_space = false;
          }
          '>' if self.line_has_only_space => {
            self.line_state = LineState::Quote;
            self.line_has_only_space = false;
          }
          ch if !ch.is_whitespace() => {
            self.line_state = LineState::Plain;
            self.line_has_only_space = false;
          }
          _ => {}
        },
        _ => {}
      }
    }

    if !ch.is_whitespace() {
      self.line_has_only_space = false;
    }

    let mut buf = [0u8; 4];
    let slice = ch.encode_utf8(&mut buf);
    self.emit_with_current_style(slice)?;
    self.last_char_was_newline = false;

    Ok(())
  }

  fn emit_with_current_style(&mut self, text: &str) -> io::Result<()> {
    if let Some(style) = self.current_style() {
      write!(self.writer, "{}{}{}", style.render(), text, style.render_reset())
    } else {
      self.writer.write_all(text.as_bytes())
    }
  }

  fn current_style(&self) -> Option<Style> {
    if !self.markdown_enabled {
      return None;
    }

    let mut style = Style::new();
    let mut has_style = false;

    if self.in_code_block {
      style = code_style();
      has_style = true;
    } else {
      match self.line_state {
        LineState::Heading => {
          style = heading_style();
          has_style = true;
        }
        LineState::Bullet => {
          style = bullet_style();
          has_style = true;
        }
        LineState::Quote => {
          style = quote_style();
          has_style = true;
        }
        _ => {}
      }
    }

    if self.inline_bold {
      style = style.bold();
      has_style = true;
    }

    if has_style {
      Some(style)
    } else {
      None
    }
  }

  fn write_fence(&mut self) -> io::Result<()> {
    if self.markdown_enabled {
      let style = fence_style();
      write!(self.writer, "{}{}{}", style.render(), "```", style.render_reset())?;
    } else {
      self.writer.write_all(b"```")?;
    }
    self.last_char_was_newline = false;
    Ok(())
  }
}

fn code_style() -> Style {
  Style::new().fg_color(Some(AnsiColor::BrightMagenta.into()))
}

fn heading_style() -> Style {
  Style::new().fg_color(Some(AnsiColor::BrightBlue.into())).bold()
}

fn bullet_style() -> Style {
  Style::new().fg_color(Some(AnsiColor::BrightGreen.into())).bold()
}

fn quote_style() -> Style {
  Style::new().fg_color(Some(AnsiColor::BrightBlack.into())).dimmed()
}

fn fence_style() -> Style {
  Style::new().fg_color(Some(AnsiColor::BrightBlack.into()))
}
