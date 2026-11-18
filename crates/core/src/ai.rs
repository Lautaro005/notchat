//! Local AI providers. Everything speaks the OpenAI-compatible chat API on
//! `localhost`; no request ever leaves the machine from this module.

use crate::error::{Error, Result};
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::future::Future;
use std::path::Path;
use std::pin::Pin;
use std::time::Duration;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProviderKind {
    Ollama,
    LmStudio,
    LlamaCpp,
    /// The llama.cpp server Notchat downloads and runs itself.
    Managed,
}

#[derive(Debug, Clone, Serialize)]
pub struct ProviderInfo {
    pub kind: ProviderKind,
    pub name: &'static str,
    pub base_url: String,
    pub installed: bool,
    pub running: bool,
    pub models: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct AiConfig {
    pub provider: Option<ProviderKind>,
    pub base_url: String,
    pub model: String,
    /// Ask before the assistant sends a WhatsApp message on request.
    #[serde(default = "yes")]
    pub confirm_send: bool,
    /// Allow the assistant chat to search and read the web.
    #[serde(default = "yes")]
    pub web_access: bool,
}

fn yes() -> bool {
    true
}

/// Local ports to probe for each provider, for setups that moved them off
/// the defaults. Always on 127.0.0.1.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct ProviderPorts {
    pub ollama: u16,
    pub lm_studio: u16,
    pub llama_cpp: u16,
}

impl Default for ProviderPorts {
    fn default() -> Self {
        Self {
            ollama: 11434,
            lm_studio: 1234,
            llama_cpp: 8080,
        }
    }
}

impl ProviderPorts {
    /// Ports 0 and below 1024 (privileged) are refused.
    pub fn validate(&self) -> Result<()> {
        if [self.ollama, self.lm_studio, self.llama_cpp]
            .iter()
            .all(|p| *p >= 1024)
        {
            Ok(())
        } else {
            Err(Error::Invalid("port".into()))
        }
    }
}

impl AiConfig {
    pub fn is_ready(&self) -> bool {
        self.provider.is_some() && !self.model.is_empty() && check_local_url(&self.base_url).is_ok()
    }
}

/// Accepts only `http://` URLs on a loopback host. This is what keeps chat
/// content on the machine: a remote endpoint can never be configured.
pub fn check_local_url(raw: &str) -> Result<url::Url> {
    let url = url::Url::parse(raw).map_err(|_| Error::Invalid("provider url".into()))?;
    let local = match url.host() {
        Some(url::Host::Domain(d)) => d == "localhost",
        Some(url::Host::Ipv4(ip)) => ip.is_loopback(),
        Some(url::Host::Ipv6(ip)) => ip.is_loopback(),
        None => false,
    };
    if url.scheme() != "http" || !local {
        return Err(Error::Blocked("AI provider must be on localhost".into()));
    }
    Ok(url)
}

fn app_installed(names: &[&str]) -> bool {
    let home = std::env::var("HOME").unwrap_or_default();
    names.iter().any(|n| {
        Path::new("/Applications").join(n).exists()
            || Path::new(&home).join("Applications").join(n).exists()
    })
}

fn bin_installed(names: &[&str]) -> bool {
    let home = std::env::var("HOME").unwrap_or_default();
    let dirs = [
        "/opt/homebrew/bin".to_string(),
        "/usr/local/bin".to_string(),
        format!("{home}/.lmstudio/bin"),
        format!("{home}/.local/bin"),
    ];
    names
        .iter()
        .any(|n| dirs.iter().any(|d| Path::new(d).join(n).exists()))
}

async fn list_models(base_url: &str) -> Option<Vec<String>> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_millis(900))
        .no_proxy()
        .build()
        .ok()?;
    let body: Value = client
        .get(format!("{base_url}/v1/models"))
        .send()
        .await
        .ok()?
        .error_for_status()
        .ok()?
        .json()
        .await
        .ok()?;
    let mut models: Vec<String> = body
        .get("data")?
        .as_array()?
        .iter()
        .filter_map(|m| m.get("id")?.as_str().map(str::to_string))
        .collect();
    models.sort();
    Some(models)
}

/// Detects installed and running providers. `managed_url` is the address of
/// Notchat's own llama.cpp server when it is running.
pub async fn detect(
    managed_url: Option<String>,
    managed_installed: bool,
    ports: ProviderPorts,
) -> Vec<ProviderInfo> {
    let candidates = [
        (
            ProviderKind::Ollama,
            "Ollama",
            format!("http://127.0.0.1:{}", ports.ollama),
            app_installed(&["Ollama.app"]) || bin_installed(&["ollama"]),
        ),
        (
            ProviderKind::LmStudio,
            "LM Studio",
            format!("http://127.0.0.1:{}", ports.lm_studio),
            app_installed(&["LM Studio.app"]) || bin_installed(&["lms"]),
        ),
        (
            ProviderKind::LlamaCpp,
            "llama.cpp",
            format!("http://127.0.0.1:{}", ports.llama_cpp),
            app_installed(&["Llama.app", "LlamaBarn.app"]) || bin_installed(&["llama-server"]),
        ),
        (
            ProviderKind::Managed,
            "Notchat llama.cpp",
            managed_url.clone().unwrap_or_default(),
            managed_installed,
        ),
    ];
    let probes = candidates.iter().map(|(_, _, url, _)| {
        let url = url.clone();
        async move {
            if url.is_empty() {
                None
            } else {
                list_models(&url).await
            }
        }
    });
    let results = futures_util::future::join_all(probes).await;
    candidates
        .into_iter()
        .zip(results)
        .map(|((kind, name, base_url, installed), models)| ProviderInfo {
            kind,
            name,
            base_url,
            installed: installed || models.is_some(),
            running: models.is_some(),
            models: models.unwrap_or_default(),
        })
        .collect()
}

// ---- chat API ------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct ToolCall {
    pub id: String,
    pub name: String,
    /// Raw JSON arguments as produced by the model.
    pub arguments: String,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct ChatMsg {
    /// "system", "user", "assistant" or "tool"
    pub role: String,
    pub content: String,
    /// Image attachments as `data:` URLs (user turns only).
    pub images: Vec<String>,
    pub tool_calls: Vec<ToolCall>,
    pub tool_call_id: Option<String>,
}

impl ChatMsg {
    pub fn new(role: &str, content: impl Into<String>) -> Self {
        Self {
            role: role.to_string(),
            content: content.into(),
            ..Default::default()
        }
    }

    fn to_json(&self) -> Value {
        let mut m = json!({ "role": self.role });
        if self.images.is_empty() {
            m["content"] = json!(self.content);
        } else {
            let mut parts = vec![json!({ "type": "text", "text": self.content })];
            parts.extend(
                self.images
                    .iter()
                    .map(|url| json!({ "type": "image_url", "image_url": { "url": url } })),
            );
            m["content"] = json!(parts);
        }
        if !self.tool_calls.is_empty() {
            m["tool_calls"] = self
                .tool_calls
                .iter()
                .map(|c| {
                    json!({
                        "id": c.id,
                        "type": "function",
                        "function": { "name": c.name, "arguments": c.arguments }
                    })
                })
                .collect();
        }
        if let Some(id) = &self.tool_call_id {
            m["tool_call_id"] = json!(id);
        }
        m
    }
}

pub type TokenSink<'a> = &'a (dyn Fn(&str) + Send + Sync);
pub type BoxFut<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

/// A chat-completion backend. Abstracted so the agent can be tested offline.
pub trait Llm: Send + Sync {
    fn chat<'a>(
        &'a self,
        messages: &'a [ChatMsg],
        tools: &'a [Value],
        on_token: TokenSink<'a>,
    ) -> BoxFut<'a, Result<ChatMsg>>;
}

pub struct LocalLlm {
    base_url: String,
    model: String,
    client: reqwest::Client,
}

impl LocalLlm {
    pub fn new(base_url: &str, model: &str) -> Result<Self> {
        check_local_url(base_url)?;
        if model.is_empty() || model.len() > 300 {
            return Err(Error::Invalid("model".into()));
        }
        let client = reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(5))
            .read_timeout(Duration::from_secs(180))
            .no_proxy()
            .build()
            .map_err(|e| Error::Ai(e.to_string()))?;
        Ok(Self {
            base_url: base_url.trim_end_matches('/').to_string(),
            model: model.to_string(),
            client,
        })
    }

    pub fn from_config(cfg: &AiConfig) -> Result<Self> {
        if cfg.provider.is_none() || cfg.model.is_empty() {
            return Err(Error::Ai("no AI model selected".into()));
        }
        Self::new(&cfg.base_url, &cfg.model)
    }

    async fn request(
        &self,
        messages: &[ChatMsg],
        tools: &[Value],
        on_token: TokenSink<'_>,
    ) -> Result<ChatMsg> {
        let mut body = json!({
            "model": self.model,
            "stream": true,
            "messages": messages.iter().map(ChatMsg::to_json).collect::<Vec<_>>(),
        });
        if !tools.is_empty() {
            body["tools"] = json!(tools);
        }
        let resp = self
            .client
            .post(format!("{}/v1/chat/completions", self.base_url))
            .json(&body)
            .send()
            .await
            .map_err(|e| Error::Ai(format!("provider unreachable: {e}")))?;
        if !resp.status().is_success() {
            let status = resp.status().as_u16();
            let detail: String = resp
                .text()
                .await
                .unwrap_or_default()
                .chars()
                .take(300)
                .collect();
            return Err(Error::Ai(format!("provider returned {status}: {detail}")));
        }
        let mut acc = StreamAccumulator::default();
        let mut buf: Vec<u8> = Vec::new();
        let mut stream = resp.bytes_stream();
        while let Some(chunk) = stream.next().await {
            buf.extend_from_slice(&chunk.map_err(|e| Error::Ai(e.to_string()))?);
            while let Some(pos) = buf.iter().position(|b| *b == b'\n') {
                let line: Vec<u8> = buf.drain(..=pos).collect();
                if let Some(token) = acc.push_line(&String::from_utf8_lossy(&line)) {
                    on_token(&token);
                }
            }
        }
        if !buf.is_empty() {
            if let Some(token) = acc.push_line(&String::from_utf8_lossy(&buf)) {
                on_token(&token);
            }
        }
        Ok(acc.finish())
    }
}

impl Llm for LocalLlm {
    fn chat<'a>(
        &'a self,
        messages: &'a [ChatMsg],
        tools: &'a [Value],
        on_token: TokenSink<'a>,
    ) -> BoxFut<'a, Result<ChatMsg>> {
        Box::pin(self.request(messages, tools, on_token))
    }
}

/// Assembles a streamed chat completion from server-sent event lines.
#[derive(Default)]
pub struct StreamAccumulator {
    content: String,
    calls: Vec<ToolCall>,
}

impl StreamAccumulator {
    /// Feeds one SSE line; returns newly produced visible text, if any.
    pub fn push_line(&mut self, line: &str) -> Option<String> {
        let data = line.trim().strip_prefix("data:")?.trim();
        if data == "[DONE]" {
            return None;
        }
        let value: Value = serde_json::from_str(data).ok()?;
        let delta = value.get("choices")?.get(0)?.get("delta")?;
        if let Some(calls) = delta.get("tool_calls").and_then(Value::as_array) {
            for call in calls {
                let index = call.get("index").and_then(Value::as_u64).unwrap_or(0) as usize;
                while self.calls.len() <= index {
                    self.calls.push(ToolCall::default());
                }
                let slot = &mut self.calls[index];
                if let Some(id) = call.get("id").and_then(Value::as_str) {
                    slot.id = id.to_string();
                }
                if let Some(f) = call.get("function") {
                    if let Some(name) = f.get("name").and_then(Value::as_str) {
                        slot.name.push_str(name);
                    }
                    if let Some(args) = f.get("arguments").and_then(Value::as_str) {
                        slot.arguments.push_str(args);
                    }
                }
            }
        }
        let token = delta.get("content").and_then(Value::as_str)?;
        if token.is_empty() {
            return None;
        }
        self.content.push_str(token);
        Some(token.to_string())
    }

    pub fn finish(mut self) -> ChatMsg {
        self.calls.retain(|c| !c.name.is_empty());
        for (i, call) in self.calls.iter_mut().enumerate() {
            if call.id.is_empty() {
                call.id = format!("call_{i}");
            }
        }
        ChatMsg {
            role: "assistant".into(),
            content: strip_think(&self.content),
            tool_calls: self.calls,
            ..Default::default()
        }
    }
}

/// Removes `<think>…</think>` reasoning blocks some local models emit inline.
pub fn strip_think(text: &str) -> String {
    let mut out = String::new();
    let mut rest = text;
    while let Some(start) = rest.find("<think>") {
        out.push_str(&rest[..start]);
        match rest[start..].find("</think>") {
            Some(end) => rest = &rest[start + end + "</think>".len()..],
            None => {
                rest = "";
                break;
            }
        }
    }
    out.push_str(rest);
    out.trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_loopback_providers_are_accepted() {
        assert!(check_local_url("http://127.0.0.1:11434").is_ok());
        assert!(check_local_url("http://localhost:1234").is_ok());
        assert!(check_local_url("http://[::1]:8080").is_ok());
        assert!(check_local_url("https://api.openai.com").is_err());
        assert!(check_local_url("http://192.168.1.10:11434").is_err());
        assert!(check_local_url("http://localhost.evil.com").is_err());
        assert!(check_local_url("https://localhost:1234").is_err());
        assert!(check_local_url("nope").is_err());
    }

    #[test]
    fn ports_validate() {
        assert!(ProviderPorts::default().validate().is_ok());
        let bad = ProviderPorts {
            ollama: 80,
            ..Default::default()
        };
        assert!(bad.validate().is_err());
    }

    #[test]
    fn stream_accumulates_text_and_tool_calls() {
        let mut acc = StreamAccumulator::default();
        assert_eq!(
            acc.push_line(r#"data: {"choices":[{"delta":{"content":"Ho"}}]}"#),
            Some("Ho".into())
        );
        assert_eq!(
            acc.push_line(r#"data: {"choices":[{"delta":{"content":"la"}}]}"#),
            Some("la".into())
        );
        assert_eq!(acc.push_line(": keep-alive"), None);
        assert_eq!(acc.push_line("data: not json"), None);
        acc.push_line(
            r#"data: {"choices":[{"delta":{"tool_calls":[{"index":0,"id":"c1","function":{"name":"read_chat","arguments":"{\"ch"}}]}}]}"#,
        );
        acc.push_line(
            r#"data: {"choices":[{"delta":{"tool_calls":[{"index":0,"function":{"arguments":"at\":\"Ana\"}"}}]}}]}"#,
        );
        assert_eq!(acc.push_line("data: [DONE]"), None);
        let msg = acc.finish();
        assert_eq!(msg.content, "Hola");
        assert_eq!(msg.tool_calls.len(), 1);
        assert_eq!(msg.tool_calls[0].name, "read_chat");
        assert_eq!(msg.tool_calls[0].arguments, r#"{"chat":"Ana"}"#);
    }

    #[test]
    fn think_blocks_are_removed() {
        assert_eq!(strip_think("<think>plan</think>Hola"), "Hola");
        assert_eq!(strip_think("a<think>x</think>b<think>y</think>c"), "abc");
        assert_eq!(strip_think("Hola <think>unfinished"), "Hola");
        assert_eq!(strip_think("plain"), "plain");
    }

    #[test]
    fn message_json_shapes() {
        let mut m = ChatMsg::new("user", "hi");
        assert_eq!(m.to_json(), json!({"role":"user","content":"hi"}));
        m.images.push("data:image/png;base64,AAAA".into());
        assert_eq!(m.to_json()["content"][1]["type"], "image_url");
        let t = ChatMsg {
            role: "tool".into(),
            content: "ok".into(),
            tool_call_id: Some("c1".into()),
            ..Default::default()
        };
        assert_eq!(t.to_json()["tool_call_id"], "c1");
    }
}
