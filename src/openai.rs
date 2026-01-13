use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use crate::config::Config;
use crate::twitter::Tweet;

#[derive(Debug, Serialize)]
struct ChatRequest {
    model: String,
    messages: Vec<Message>,
    max_tokens: u32,
    temperature: f32,
}

#[derive(Debug, Serialize, Deserialize)]
struct Message {
    role: String,
    content: String,
}

#[derive(Debug, Deserialize)]
struct ChatResponse {
    choices: Vec<Choice>,
}

#[derive(Debug, Deserialize)]
struct Choice {
    message: Message,
}

/// Summarize tweets using OpenAI's API
pub async fn summarize_tweets(config: &Config, tweets: &[Tweet]) -> Result<String> {
    let client = reqwest::Client::new();

    // Format tweets for the prompt
    let tweets_text = tweets
        .iter()
        .enumerate()
        .map(|(i, t)| format!("{}. {}", i + 1, t.text))
        .collect::<Vec<_>>()
        .join("\n\n");

    let system_prompt = r#"You are a helpful assistant that summarizes Twitter/X content. 
Your task is to create a concise, informative summary of the tweets provided.

Guidelines:
- Group related topics together
- Highlight the most important or trending discussions
- Keep the summary scannable with bullet points
- Include key insights or interesting takes
- Keep the total summary under 500 words
- Use emojis sparingly to make it visually appealing for WhatsApp"#;

    let user_prompt = format!(
        "Please summarize these {} tweets from my Twitter list:\n\n{}",
        tweets.len(),
        tweets_text
    );

    let request = ChatRequest {
        model: config.openai_model.clone(),
        messages: vec![
            Message {
                role: "system".to_string(),
                content: system_prompt.to_string(),
            },
            Message {
                role: "user".to_string(),
                content: user_prompt,
            },
        ],
        max_tokens: 1000,
        temperature: 0.7,
    };

    let response = client
        .post("https://api.openai.com/v1/chat/completions")
        .header("Authorization", format!("Bearer {}", config.openai_api_key))
        .header("Content-Type", "application/json")
        .json(&request)
        .send()
        .await
        .context("Failed to send request to OpenAI API")?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        anyhow::bail!("OpenAI API error ({}): {}", status, body);
    }

    let chat_response: ChatResponse = response
        .json()
        .await
        .context("Failed to parse OpenAI response")?;

    let summary = chat_response
        .choices
        .first()
        .map(|c| c.message.content.clone())
        .unwrap_or_else(|| "No summary generated".to_string());

    Ok(summary)
}
