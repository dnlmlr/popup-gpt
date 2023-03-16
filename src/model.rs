use serde::{Deserialize, Serialize};

pub const DEFAULT_MODEL: &str = "gpt-3.5-turbo";

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    System,
    Assistant,
    User,
}

/// A chat single message than can occur in CompletionRequest or CompletionResponse
///
/// - https://platform.openai.com/docs/guides/chat/response-format
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Message {
    pub role: Role,
    pub content: String,
}

/// A Chat Completion Request
///
/// - https://platform.openai.com/docs/api-reference/chat/create
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct CompletionRequest {
    /// ID of the model to use. Currently, only `gpt-3.5-turbo` and gpt-3.5-turbo-0301 are
    /// supported.
    pub model: String,

    /// The messages to generate chat completions for, in the chat format.
    pub messages: Vec<Message>,

    /// What sampling temperature to use, between 0 and 2. Higher values like 0.8 will make the
    /// output more random, while lower values like 0.2 will make it more focused and deterministic.
    ///
    /// We generally recommend altering this or `top_p` but not both.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f32>,

    /// An alternative to sampling with temperature, called nucleus sampling, where the model
    /// considers the results of the tokens with top_p probability mass. So 0.1 means only the
    /// tokens comprising the top 10% probability mass are considered.
    ///
    /// We generally recommend altering this or `temperature` but not both.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top_p: Option<f32>,

    /// How many chat completion choices to generate for each input message.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub n: Option<u32>,

    /// If set, partial message deltas will be sent, like in ChatGPT. Tokens will be sent as
    /// data-only server-sent events as they become available, with the stream terminated by a
    /// `data: [DONE]` message.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stream: Option<bool>,

    // stop: Option<String>
    /// The maximum number of tokens allowed for the generated answer. By default, the number of
    /// tokens the model can return will be (4096 - prompt tokens).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<u64>,

    /// Number between -2.0 and 2.0. Positive values penalize new tokens based on whether they
    /// appear in the text so far, increasing the model's likelihood to talk about new topics.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub presence_penalty: Option<f32>,

    /// Number between -2.0 and 2.0. Positive values penalize new tokens based on their existing
    /// frequency in the text so far, decreasing the model's likelihood to repeat the same line
    /// verbatim.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub frequency_penalty: Option<f32>,

    /// A unique identifier representing your end-user, which can help OpenAI to monitor and detect
    /// abuse.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user: Option<String>,
}

/// The API Response to a completion Request. This contains the completed chat messages.
///
/// - https://platform.openai.com/docs/guides/chat/response-format
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct CompletionResponse {
    pub id: String,
    pub object: String,
    pub created: u64,
    pub choices: Vec<Choice>,
    pub usage: Option<Usage>,
}

/// A single variant of possible completions. A CompletionResponse can contain multiple different
/// completion variants
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct Choice {
    pub index: u64,
    pub message: Option<Message>,
    pub delta: Option<MessageDelta>,
    pub finish_reason: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct MessageDelta {
    pub role: Option<Role>,
    pub content: Option<String>,
}

/// Token Usage of the associated Request & Response
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct Usage {
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
    pub total_tokens: u32,
}

impl Message {
    pub fn system(msg: impl AsRef<str>) -> Self {
        Self {
            role: Role::System,
            content: msg.as_ref().to_string(),
        }
    }
    pub fn user(msg: impl AsRef<str>) -> Self {
        Self {
            role: Role::User,
            content: msg.as_ref().to_string(),
        }
    }
    pub fn assistant(msg: impl AsRef<str>) -> Self {
        Self {
            role: Role::Assistant,
            content: msg.as_ref().to_string(),
        }
    }
}

impl CompletionResponse {
    pub fn primary_response(&self) -> Option<&str> {
        self.choices
            .first()
            .map(|it| it.message.as_ref().map(|msg| msg.content.as_str()))
            .flatten()
    }

    pub fn used_tokens(&self) -> Option<u32> {
        self.usage.as_ref().map(|usage| usage.total_tokens)
    }

    pub fn merge_delta(&mut self, other: Self) {
        for choice in other.choices {
            while self.choices.len() <= choice.index as usize {
                self.choices.push(Choice::default());
            }

            let own_choice = &mut self.choices[choice.index as usize];

            if let Some(delta) = choice.delta {
                if let Some(role) = delta.role {
                    own_choice.message = Some(Message {
                        role,
                        content: String::new(),
                    });
                }
                if let Some(content) = delta.content {
                    own_choice
                        .message
                        .as_mut()
                        .unwrap()
                        .content
                        .push_str(&content);
                }
            }
        }
    }
}
