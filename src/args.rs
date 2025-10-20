use argh::FromArgs;

#[derive(FromArgs)]
///  GmnTop 工具，用于与 Gemini API 交互生成内容.
///  可以根据用户提供的提示生成文本响应.
///
///  例如：
///  gmn --prompt "写一首关于秋天的诗" --temperature 0.3 --max-tokens 4096
///  gmn --stdin < file.txt
pub struct GmnTop {
  /// a custom prompt, default is guess and analyse
  #[argh(option, short = 'p')]
  pub prompt: Option<String>,

  /// a model name, default is gemini-2.5-flash
  #[argh(option, short = 'm')]
  pub model: Option<String>,

  /// temperature for generation (0.0-1.0), default is 0.3
  #[argh(option, short = 't')]
  pub temperature: Option<f32>,

  /// maximum output tokens, default is 8192
  #[argh(option)]
  pub max_tokens: Option<i32>,

  #[argh(switch)]
  /// read from stdin, mostly from pipe
  pub stdin: bool,

  /// input file path
  #[argh(positional)]
  pub file: Option<String>,
}
