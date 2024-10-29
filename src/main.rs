mod args;

use std::io::Read;

use args::GmnTop;
use genai::chat::printer::{print_chat_stream, PrintChatStreamOptions};
use genai::chat::{ChatMessage, ChatRequest};
use genai::Client;

const MODEL_GEMINI: &str = "gemini-1.5-flash-latest";
const MODEL_CLAUDE: &str = "claude-3-5-sonnet-20240620";

const GEMINI_ENV_NAME: &str = "GEMINI_API_KEY";
const CLAUDE_ENV_NAME: &str = "ANTHROPIC_API_KEY";

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
  let options: GmnTop = argh::from_env();

  if options.model == Some("claude".to_string()) {
    if std::env::var(CLAUDE_ENV_NAME).is_err() {
      eprintln!("Please set the environment variable `{}`", CLAUDE_ENV_NAME);
      std::process::exit(1);
    }
  } else if std::env::var(GEMINI_ENV_NAME).is_err() {
    eprintln!("Please set the environment variable `{}`", GEMINI_ENV_NAME);
    std::process::exit(1);
  }

  let file_content = if options.stdin {
    let mut buffer = Vec::new();
    std::io::stdin().read_to_end(&mut buffer)?;
    // turn string
    String::from_utf8(buffer)?
    // println!("Read from stdin: {}", ret);
  } else if let Some(file) = &options.file {
    std::fs::read_to_string(file)?
  } else {
    eprintln!("Please provide a file or use stdin");
    std::process::exit(1);
  };

  let prompt = match options.prompt.as_deref() {
    Some("review") => "审查该代码. 用中文回复. 给出一些优化建议.".to_string(),
    Some(prompt) => prompt.to_owned(),
    None => "分析文件内容, 尝试给出一些有用的信息. 用中文回复.".to_string(),
  };

  let chat_req = ChatRequest::new(vec![
    // -- Messages (de/activate to see the differences)
    ChatMessage::system(prompt),
    ChatMessage::user(file_content),
  ]);

  let client = Client::default();

  let print_options = PrintChatStreamOptions::from_print_events(false);

  let model = match options.model.as_deref() {
    Some("claude") => MODEL_CLAUDE,
    _ => MODEL_GEMINI,
  };

  let chat_res = client.exec_chat_stream(model, chat_req.clone(), None).await?;
  print_chat_stream(chat_res, Some(&print_options)).await?;

  Ok(())
}
