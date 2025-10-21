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
  line_state: LineState,
  line_has_only_space: bool,
  last_char_was_newline: bool,
}

impl<W: Write> StreamingFormatter<W> {
  pub fn new(writer: W) -> Self {
    Self {
      writer,
      in_code_block: false,
      pending_backticks: 0,
      line_state: LineState::Unknown,
      line_has_only_space: true,
      last_char_was_newline: true,
    }
  }

  pub fn write_chunk(&mut self, chunk: &str) -> io::Result<()> {
    for ch in chunk.chars() {
      if ch == '`' {
        self.handle_backtick()?;
      } else {
        if self.pending_backticks > 0 {
          self.flush_literal_backticks()?;
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

    if !self.last_char_was_newline {
      self.writer.write_all(b"\n")?;
      self.last_char_was_newline = true;
    }

    self.writer.flush()
  }

  fn handle_backtick(&mut self) -> io::Result<()> {
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

  fn write_char(&mut self, ch: char) -> io::Result<()> {
    if ch == '\r' {
      return Ok(());
    }

    if ch == '\n' {
      self.writer.write_all(b"\n")?;
      self.line_state = LineState::Unknown;
      self.line_has_only_space = true;
      self.last_char_was_newline = true;
      return Ok(());
    }

    if !self.in_code_block {
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
    if self.in_code_block {
      return Some(code_style());
    }

    match self.line_state {
      LineState::Heading => Some(heading_style()),
      LineState::Bullet => Some(bullet_style()),
      LineState::Quote => Some(quote_style()),
      _ => None,
    }
  }

  fn write_fence(&mut self) -> io::Result<()> {
    let style = fence_style();
    write!(self.writer, "{}{}{}", style.render(), "```", style.render_reset())?;
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
