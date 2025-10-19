use display_error_chain::DisplayErrorChain;
use futures::TryStreamExt;
use gemini_rust::GenerationConfig;
use std::env;
use std::fs;
use std::io::{self, Read, Write};
use std::process::ExitCode;
use tracing::info;

mod args;
use args::GmnTop;

#[tokio::main]
async fn main() -> ExitCode {
  tracing_subscriber::fmt()
    .with_env_filter(
      tracing_subscriber::EnvFilter::builder()
        .with_default_directive(tracing::level_filters::LevelFilter::INFO.into())
        .from_env_lossy(),
    )
    .init();

  match do_main().await {
    Ok(()) => ExitCode::SUCCESS,
    Err(e) => {
      let error_chain = DisplayErrorChain::new(e.as_ref());
      tracing::error!(error.debug = ?e, error.chained = %error_chain, "execution failed");
      ExitCode::FAILURE
    }
  }
}

async fn do_main() -> Result<(), Box<dyn std::error::Error>> {
  let args: GmnTop = argh::from_env();

  // Determine model to use
  let model_name = args.model.as_deref().unwrap_or("gemini-2.5-flash");

  // Check for required environment variables
  if env::var("GEMINI_API_KEY").is_err() {
    eprintln!("Error: GEMINI_API_KEY environment variable is required");
    return Err("GEMINI_API_KEY environment variable not set".into());
  }

  // Get API key from environment variable
  let api_key = env::var("GEMINI_API_KEY").expect("GEMINI_API_KEY environment variable not set");

  // Get base URL from environment variable or use Google's official API
  let custom_base_url = env::var("GEMINI_BASE_URL").ok();
  if let Some(url) = &custom_base_url {
    info!(base_url = %url, "Using custom base URL from GEMINI_BASE_URL");
  } else {
    info!("GEMINI_BASE_URL not set, using Google's official API endpoint");
  }

  // Create client using GeminiBuilder (for 1.5.1 version)
  let mut builder = gemini_rust::GeminiBuilder::new(api_key);

  // Apply custom base URL if provided
  if let Some(url) = custom_base_url {
    // 确保 URL 结尾有 v1beta/
    let url = if url.ends_with('/') {
      if url.ends_with("v1beta/") {
        url.to_string()
      } else {
        format!("{}v1beta/", url)
      }
    } else {
      format!("{}/v1beta/", url)
    };
    builder = builder.with_base_url(url.parse()?);
  }

  // Set model and build client
  let client = builder.with_model(format!("models/{}", model_name)).build()?;

  // Read input from file or stdin
  let input = if args.stdin {
    let mut buffer = String::new();
    io::stdin().read_to_string(&mut buffer)?;
    buffer
  } else if let Some(file_path) = args.file {
    fs::read_to_string(file_path)?
  } else {
    eprintln!("Error: Please provide input via --stdin or specify a file");
    return Err("No input provided".into());
  };

  // Prepare the message with optional prompt
  let user_message = if let Some(prompt) = args.prompt {
    format!("{}\n\n{}", prompt, input)
  } else {
    input
  };

  // Execute the request in streaming mode
  info!("Sending streaming request to Gemini API with model: {}", model_name);

  // Create a streaming request
  let mut stream = match client
    .generate_content()
    .with_system_instruction("You are a helpful assistant.")
    .with_user_message(user_message)
    .with_generation_config(GenerationConfig {
      temperature: Some(0.7),
      max_output_tokens: Some(2048),
      ..Default::default()
    })
    .execute_stream()
    .await
  {
    Ok(stream) => stream,
    Err(e) => {
      tracing::error!(error = ?e, "Failed to get streaming response from Gemini API");
      return Err(format!("API streaming request failed: {}", e).into());
    }
  };

  // Process the stream chunks as they arrive
  info!("Receiving streaming response from Gemini API");
  let mut full_response = String::new();

  while let Some(chunk) = stream.try_next().await? {
    let text = chunk.text();
    print!("{}", text);
    full_response.push_str(&text);

    // Flush stdout to ensure immediate display
    io::stdout().flush()?;
  }

  println!(); // Add a newline at the end
  info!(response_length = full_response.len(), "Streaming response completed");

  Ok(())
}
