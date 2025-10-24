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
  inline_code: bool,
  markdown_enabled: bool,
  current_active_style: Option<Style>,
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
      inline_code: false,
      markdown_enabled,
      current_active_style: None,
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

    // 重置所有样式状态
    if self.current_active_style.is_some() {
      write!(self.writer, "{}", Style::new().render_reset())?;
      self.current_active_style = None;
    }

    self.inline_bold = false;
    self.inline_code = false;

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
    } else if self.pending_backticks == 1 && !self.in_code_block {
      // Handle single backtick for inline code
      self.inline_code = !self.inline_code;
      self.pending_backticks = 0;
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

    if self.markdown_enabled && !self.in_code_block && self.line_state == LineState::Unknown {
      match ch {
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
    let desired_style = self.current_style();

    // 检查是否需要改变样式
    if self.current_active_style != desired_style {
      // 如果当前有活跃样式，先重置
      if self.current_active_style.is_some() {
        write!(self.writer, "\u{1b}[0m")?;
      }

      // 应用新样式
      if let Some(style) = desired_style {
        write!(self.writer, "{}", style.render())?;
      }

      self.current_active_style = desired_style;
    }

    // 写入文本
    self.writer.write_all(text.as_bytes())
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
    } else if self.inline_code {
      style = inline_code_style();
      has_style = true;
    } else {
      match self.line_state {
        LineState::Heading => {
          style = heading_style();
          has_style = true;
        }
        LineState::Bullet => {
          let bullet_style_val = bullet_style();
          // 检查是否为默认样式（无颜色无效果）
          let default_style = Style::new();
          if bullet_style_val != default_style {
            style = bullet_style_val;
            has_style = true;
          }
        }
        LineState::Quote => {
          style = quote_style();
          has_style = true;
        }
        _ => {}
      }
    }

    if self.inline_bold && !self.inline_code {
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
      write!(self.writer, "{}```{}", style.render(), style.render_reset())?;
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

fn inline_code_style() -> Style {
  Style::new().fg_color(Some(AnsiColor::BrightYellow.into())).bold()
}

fn heading_style() -> Style {
  Style::new().fg_color(Some(AnsiColor::BrightBlue.into())).bold()
}

fn bullet_style() -> Style {
  Style::new() // 使用默认样式，无颜色，让列表项显示为默认白色
}

fn quote_style() -> Style {
  Style::new().fg_color(Some(AnsiColor::BrightBlack.into())).dimmed()
}

fn fence_style() -> Style {
  Style::new().fg_color(Some(AnsiColor::BrightBlack.into()))
}

#[cfg(test)]
mod tests {
  use super::*;
  use std::io::Cursor;

  fn format_text(input: &str, styled: bool) -> String {
    let mut output = Vec::new();
    let mut formatter = if styled {
      StreamingFormatter::styled(Cursor::new(&mut output))
    } else {
      StreamingFormatter::plain(Cursor::new(&mut output))
    };

    formatter.write_chunk(input).unwrap();
    formatter.finish().unwrap();

    String::from_utf8(output).unwrap()
  }

  #[test]
  fn test_single_inline_code() {
    let input = "This is `code` text";
    let result = format_text(input, true);
    println!("Input: {}", input);
    println!("Output: {:?}", result);

    // 应该不包含反引号，但包含 ANSI 颜色代码
    assert!(!result.contains("`"));
    assert!(result.contains("code"));
  }

  #[test]
  fn test_multiple_inline_codes() {
    let input = "Changed from `INFO` to `WARN` level";
    let result = format_text(input, true);
    println!("Input: {}", input);
    println!("Output: {:?}", result);

    // 应该不包含反引号
    assert!(!result.contains("`"));
    assert!(result.contains("INFO"));
    assert!(result.contains("WARN"));
  }

  #[test]
  fn test_consecutive_inline_codes() {
    let input = "`first` and `second` and `third`";
    let result = format_text(input, true);
    println!("Input: {}", input);
    println!("Output: {:?}", result);

    assert!(!result.contains("`"));
    assert!(result.contains("first"));
    assert!(result.contains("second"));
    assert!(result.contains("third"));
  }

  #[test]
  fn test_inline_code_with_bold() {
    let result = format_text("This is `code` and **bold** text", true);
    println!("Input: This is `code` and **bold** text");
    println!("Output: {:?}", result);

    assert!(result.contains("code"));
    assert!(result.contains("bold"));
    assert!(!result.contains("`"));
    assert!(!result.contains("**"));
  }

  #[test]
  fn test_inline_code_in_different_contexts() {
    let result = format_text("Normal text `code1` more text `code2` end", true);
    println!("Input: Normal text `code1` more text `code2` end");
    println!("Output: {:?}", result);

    assert!(result.contains("Normal text"));
    assert!(result.contains("code1"));
    assert!(result.contains("more text"));
    assert!(result.contains("code2"));
    assert!(result.contains("end"));
    assert!(!result.contains("`"));
  }

  #[test]
  fn test_actual_color_display() {
    use std::io::stdout;

    println!("\n=== 实际颜色显示测试 ===");

    // 直接输出到终端以查看实际颜色
    let mut formatter = StreamingFormatter::styled(stdout());

    println!("测试1: 单个行内代码");
    print!("输出: ");
    formatter.write_chunk("这是 `代码` 文本").unwrap();
    formatter.finish().unwrap();

    println!("\n测试2: 多个行内代码");
    print!("输出: ");
    let mut formatter = StreamingFormatter::styled(stdout());
    formatter.write_chunk("从 `INFO` 改为 `WARN` 级别").unwrap();
    formatter.finish().unwrap();

    println!("\n测试3: 行内代码与粗体混合");
    print!("输出: ");
    let mut formatter = StreamingFormatter::styled(stdout());
    formatter.write_chunk("这是 `代码` 和 **粗体** 文本").unwrap();
    formatter.finish().unwrap();

    println!("\n=== 测试完成 ===");
  }

  #[test]
  fn test_debug_inline_code_state() {
    let output = Cursor::new(Vec::new());
    let mut formatter = StreamingFormatter::styled(output);

    println!("\n=== 调试行内代码状态 ===");

    // 逐字符处理并打印状态
    let input = "`code`";
    for (i, ch) in input.chars().enumerate() {
      formatter.write_chunk(&ch.to_string()).unwrap();
      println!(
        "字符 '{}' (位置 {}): inline_code={}, current_style={:?}",
        ch,
        i,
        formatter.inline_code,
        formatter.current_style()
      );
    }

    formatter.finish().unwrap();
    let result = String::from_utf8(formatter.writer.into_inner()).unwrap();
    println!("最终输出: {:?}", result);
    println!("=== 调试完成 ===");
  }

  #[test]
  fn test_styled_vs_plain_mode() {
    let input = "这是 `代码` 和 **粗体** 文本";

    println!("\n=== 对比 styled 和 plain 模式 ===");
    println!("输入: {}", input);

    // 测试 styled 模式
    let styled_result = format_text(input, true);
    println!("Styled 模式输出: {:?}", styled_result);

    // 测试 plain 模式
    let plain_result = format_text(input, false);
    println!("Plain 模式输出: {:?}", plain_result);

    // 验证差异
    assert_ne!(styled_result, plain_result, "styled 和 plain 模式应该产生不同的输出");
    assert!(styled_result.contains("\u{1b}["), "styled 模式应该包含 ANSI 颜色代码");
    assert!(!plain_result.contains("\u{1b}["), "plain 模式不应该包含 ANSI 颜色代码");

    println!("=== 对比完成 ===");
  }

  #[test]
  fn test_list_with_code_colors() {
    let input = "* 这是列表项 `代码` 和 **粗体**\n* 另一个 `函数()` 调用";

    println!("\n=== 测试列表和代码颜色 ===");
    println!("输入: {}", input);

    let result = format_text(input, true);
    println!("输出: {:?}", result);

    // 验证列表项没有颜色（bullet_style 为默认样式）
    // 但代码仍然有颜色（inline_code_style 有亮黄色）
    assert!(result.contains("\u{1b}[1m\u{1b}[93m"), "代码应该有亮黄色粗体样式");

    println!("=== 测试完成 ===");
  }

  #[test]
  fn test_color_reset_after_inline_code() {
    let mut formatter = StreamingFormatter::styled(Cursor::new(Vec::new()));

    // 测试行内代码后的颜色重置
    formatter.write_chunk("这是普通文本 `代码` 后续文本\n新行文本\n").unwrap();
    formatter.finish().unwrap();

    let output = String::from_utf8(formatter.writer.into_inner()).unwrap();

    // 检查是否包含重置序列
    assert!(
      output.contains("\u{1b}[0m"),
      "输出应该包含重置序列 \\u{{1b}}[0m，但实际输出是: {:?}",
      output
    );
  }
}
