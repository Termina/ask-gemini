use display_error_chain::DisplayErrorChain;
use futures::{pin_mut, TryStreamExt};
use gemini_rust::{FinishReason, GenerationConfig};
use std::env;
use std::fs;
use std::io::{self, Read};
use std::process::ExitCode;
use std::time::Duration;
use tracing::{debug, info, warn};

mod args;
use args::GmnTop;
mod terminal;
use terminal::StreamingFormatter;

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
  let client_builder = reqwest::Client::builder().timeout(Duration::from_secs(120));

  let client = builder
    .with_http_client(client_builder)
    .with_model(format!("models/{}", model_name))
    .build()?;

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
    format!("{}\n\n-------\n\n{}", prompt, input)
  } else {
    input
  };

  // Get configuration from command line arguments with defaults
  let temperature = args.temperature.unwrap_or(0.3);
  let max_output_tokens = args.max_tokens.unwrap_or(8192);

  // Execute the request in streaming mode
  info!("Sending streaming request to Gemini API with model: {}", model_name);
  info!(
    user_message_length = user_message.len(),
    temperature = temperature,
    max_output_tokens = max_output_tokens,
    "Request parameters"
  );

  // Create a streaming request
  let stream = match client
    .generate_content()
    .with_system_instruction("You are a helpful assistant.")
    .with_user_message(user_message.clone())
    .with_generation_config(GenerationConfig {
      temperature: Some(temperature),
      max_output_tokens: Some(max_output_tokens),
      ..Default::default()
    })
    .execute_stream()
    .await
  {
    Ok(stream) => {
      info!("Successfully created streaming response");
      stream
    }
    Err(e) => {
      tracing::error!(error = ?e, "Failed to get streaming response from Gemini API");
      return Err(format!("API streaming request failed: {}", e).into());
    }
  };

  pin_mut!(stream);

  // Process the stream chunks as they arrive
  info!("Receiving streaming response from Gemini API");
  let mut full_response = String::new();
  let mut chunk_count = 0;
  let mut formatter = StreamingFormatter::new(anstream::stdout());

  // 记录请求开始时间
  let start_time = std::time::Instant::now();

  info!("Starting to process stream chunks");
  while let Some(chunk) = stream.try_next().await? {
    chunk_count += 1;
    let text = chunk.text();
    let text_len = text.len();

    // 检查 finishReason 以识别响应限制
    for candidate in &chunk.candidates {
      if let Some(finish_reason) = &candidate.finish_reason {
        match finish_reason {
          FinishReason::MaxTokens => {
            warn!("🚫 Response was truncated due to max_output_tokens limit!");
            warn!("   Current max_output_tokens: {}", max_output_tokens);
            warn!("   💡 Solution: Increase --max-tokens to {} or higher", max_output_tokens * 2);
            warn!("   💡 Example: --max-tokens {}", max_output_tokens * 2);
          }
          FinishReason::Safety => {
            warn!("🚫 Response was blocked due to safety concerns!");
            warn!("   💡 Try rephrasing your prompt to be more neutral");
            warn!("   💡 Avoid sensitive topics or explicit content");
            warn!("   💡 Consider using a different model if appropriate");
          }
          FinishReason::Recitation => {
            warn!("🚫 Response was blocked due to recitation concerns!");
            warn!("   💡 The model detected potential copyright content");
            warn!("   💡 Try asking for original content or paraphrasing");
            warn!("   💡 Avoid requesting direct quotes from copyrighted material");
          }
          FinishReason::Stop => {
            debug!("✅ Response completed normally with STOP reason");
          }
          _ => {
            warn!("ℹ️  Response finished with reason: {:?}", finish_reason);
            warn!("   💡 This is an uncommon finish reason, check Gemini API documentation");
          }
        }
      }
    }

    // 记录每个块的信息
    info!(
      chunk_number = chunk_count,
      chunk_length = text_len,
      elapsed_ms = start_time.elapsed().as_millis(),
      "Received chunk"
    );

    if text_len == 0 {
      warn!("Received empty chunk #{}", chunk_count);
    } else {
      // 记录块的前10个字符（或全部如果少于10个），安全处理 UTF-8 字符边界
      let preview = if text.chars().count() > 10 {
        let truncated: String = text.chars().take(10).collect();
        format!("{}...", truncated.replace('\n', "\\n"))
      } else {
        text.replace('\n', "\\n")
      };
      debug!(preview = %preview, "Chunk content preview");

      formatter.write_chunk(&text)?;
      full_response.push_str(&text);
    }
  }

  formatter.finish()?;

  // 记录完整响应的信息
  if full_response.is_empty() {
    warn!("⚠️  Completed streaming response but received empty content");

    // 分析可能的原因并提供调整建议
    warn!("🔍 Analyzing possible reasons for empty response:");
    warn!(
      "   Current parameters: max_output_tokens={}, temperature={}",
      max_output_tokens, temperature
    );

    if max_output_tokens < 100 {
      warn!(
        "   💡 max_output_tokens ({}) is very low. Try increasing to 500+ for meaningful responses.",
        max_output_tokens
      );
    } else if max_output_tokens < 500 {
      warn!(
        "   💡 max_output_tokens ({}) might be too low for complex responses. Consider increasing to 1000+.",
        max_output_tokens
      );
    }

    if temperature < 0.1 {
      warn!(
        "   💡 temperature ({}) is very low. Try increasing to 0.3-0.7 for more varied responses.",
        temperature
      );
    }

    warn!("   💡 Suggested parameter adjustments:");
    warn!("      - For short responses: --max-tokens 500 --temperature 0.3");
    warn!("      - For detailed responses: --max-tokens 2000 --temperature 0.5");
    warn!("      - For creative responses: --max-tokens 4000 --temperature 0.7");

    // 检查是否有任何 finishReason 信息可以帮助诊断
    warn!("   📊 If you saw any finishReason warnings above, they indicate the specific cause.");
    warn!("   📊 No finishReason warnings usually means the model chose not to respond to this input.");
  } else {
    info!(
      response_length = full_response.len(),
      chunk_count = chunk_count,
      total_time_ms = start_time.elapsed().as_millis(),
      "Streaming response completed successfully"
    );
  }

  Ok(())
}
