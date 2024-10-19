use argh::FromArgs;

#[derive(FromArgs)]
///  GmnTop 工具，用于... (描述工具的功能).
///  可以根据用户提供的提示生成... (描述生成的输出).
///
///  例如：
///  gmn-top --prompt="写一首关于秋天的诗"
///  gmn-top --stdin < file.txt
pub struct GmnTop {
  /// a cutsom prompt, default is guess and analyse
  #[argh(option, short = 'p')]
  pub prompt: Option<String>,

  /// a model name, default is gemini-1.5-flash-latest
  #[argh(option, short = 'm')]
  pub model: Option<String>,

  #[argh(switch)]
  /// read from stdin, mostly from pipe
  pub stdin: bool,

  /// an optional nickname for the pilot
  #[argh(positional)]
  pub file: Option<String>,
}
