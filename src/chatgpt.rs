use std::sync::mpsc::Sender;

use anyhow::Result;

use crate::{
    misc::SSEStream,
    model::{CompletionRequest, CompletionResponse, Message, DEFAULT_MODEL},
};

pub const CHATGPT_ENDPOINT: &str = "https://api.openai.com/v1/chat/completions";

#[derive(Debug, Clone, Default)]
pub struct ChatGPT {
    endpoint: String,
    token: String,
    assistant: Assistant,
}

#[derive(Debug, Clone)]
pub struct Assistant {
    system_msg: String,
    conversation: Vec<Message>,
}

impl Default for Assistant {
    fn default() -> Self {
        Self {
            system_msg: "You are a helpful AI assistant.".to_string(),
            conversation: Vec::new(),
        }
    }
}

impl Assistant {
    fn generate_request(&self) -> CompletionRequest {
        let mut messages = vec![Message::system(self.system_msg.clone())];
        messages.extend(self.conversation.iter().cloned());

        CompletionRequest {
            model: DEFAULT_MODEL.to_string(),
            messages,
            ..Default::default()
        }
    }
}

impl ChatGPT {
    pub fn new(token: String) -> Self {
        let endpoint = CHATGPT_ENDPOINT.to_string();
        let assistant = Assistant::default();

        Self {
            endpoint,
            token,
            assistant,
        }
    }

    fn send_request(&self, req: CompletionRequest) -> Result<ureq::Response> {
        let authorization = format!("Bearer {}", self.token);

        let resp = ureq::post(&self.endpoint)
            .set("Authorization", &authorization)
            .send_json(req)?;

        Ok(resp)
    }

    fn request(&self, req: CompletionRequest) -> Result<CompletionResponse> {
        let resp = self.send_request(req)?.into_string()?;

        println!("{}", resp);

        let resp: CompletionResponse = serde_json::from_str(&resp)?;

        Ok(resp)
    }

    fn request_stream(
        &self,
        req: CompletionRequest,
        sender: Sender<CompletionResponse>,
    ) -> Result<CompletionResponse> {
        let resp = self.send_request(req)?;

        let stream = resp.into_reader();
        let stream = SSEStream::new(stream);

        let mut response = CompletionResponse::default();

        for event in stream {
            let partial_response: CompletionResponse = serde_json::from_str(&event)?;

            response.merge_delta(partial_response.clone());
            sender.send(partial_response).unwrap();
        }

        Ok(response)
    }

    pub fn clear_conversation(&mut self) {
        self.assistant.conversation.clear();
    }

    pub fn ask(&mut self, question: impl AsRef<str>) -> Result<CompletionResponse> {
        self.assistant.conversation.push(Message::user(question));

        let req = self.assistant.generate_request();
        let resp = self.request(req)?;

        self.assistant
            .conversation
            .push(resp.choices[0].message.as_ref().unwrap().clone());

        Ok(resp)
    }

    pub fn ask_stream(
        &mut self,
        question: impl AsRef<str>,
        sender: Sender<CompletionResponse>,
    ) -> Result<CompletionResponse> {
        self.assistant.conversation.push(Message::user(question));

        let mut req = self.assistant.generate_request();
        req.stream = Some(true);
        let resp = self.request_stream(req, sender)?;

        self.assistant
            .conversation
            .push(resp.choices[0].message.as_ref().unwrap().clone());

        Ok(resp)
    }
}
