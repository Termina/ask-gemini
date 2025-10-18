use genai::chat::{ChatMessage, ChatRequest};
use genai::Client;
use std::fs;
use std::io::{self, Read};

mod args;
use args::GmnTop;

const MODEL_GEMINI: &str = "gemini-1.5-flash";
const MODEL_CLAUDE: &str = "claude-3-5-haiku-20241022";

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
  let args: GmnTop = argh::from_env();

  // Determine model to use
  let model_name = args.model.as_deref().unwrap_or("gemini-1.5-flash");
  let is_gemini = model_name.contains("gemini");
  let is_claude = model_name.contains("claude");

  // Check for required environment variables
  if std::env::var("GEMINI_API_KEY").is_err() && is_gemini {
    eprintln!("Error: GEMINI_API_KEY environment variable is required for Gemini model");
    std::process::exit(1);
  }

  if std::env::var("ANTHROPIC_API_KEY").is_err() && is_claude {
    eprintln!("Error: ANTHROPIC_API_KEY environment variable is required for Claude model");
    std::process::exit(1);
  }

  // Read input from file or stdin
  let input = if args.stdin {
    let mut buffer = String::new();
    io::stdin().read_to_string(&mut buffer)?;
    buffer
  } else if let Some(file_path) = args.file {
    fs::read_to_string(file_path)?
  } else {
    eprintln!("Error: Please provide input via --stdin or specify a file");
    std::process::exit(1);
  };

  // Create client with optional custom endpoint
  let client = if let Ok(custom_endpoint) = std::env::var("GEMINI_ENDPOINT") {
    println!("Note: Custom endpoint detected: {}", custom_endpoint);
    println!("Warning: Custom endpoint support requires additional configuration.");
    println!("For now, using default client. Custom endpoint implementation pending.");

    // TODO: Implement custom endpoint support using ServiceTargetResolver
    // This requires understanding the exact API for creating Endpoint instances
    // in the genai library version 0.4.2

    Client::default()
  } else {
    Client::default()
  };

  // Choose model based on argument
  let model = if is_gemini || is_claude {
    model_name
  } else {
    // Default to Gemini if not specified
    MODEL_GEMINI
  };

  // Prepare the message with optional prompt
  let message = if let Some(prompt) = args.prompt {
    format!("{}\n\n{}", prompt, input)
  } else {
    input
  };

  // Create chat request
  let chat_req = ChatRequest::new(vec![ChatMessage::user(message)]);

  // Execute the chat request
  let chat_res = client.exec_chat(model, chat_req, None).await?;

  // Print the response using new API
  if let Some(text) = chat_res.first_text() {
    println!("{}", text);
  } else {
    println!("No response");
  }

  Ok(())
}
