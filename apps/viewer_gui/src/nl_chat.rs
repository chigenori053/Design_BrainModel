use std::path::Path;
use std::process::Command;

use eframe::egui::{self, Color32, FontId, RichText, ScrollArea, TextStyle, Ui};

use design_cli::viewer::nl_dispatch::{NlContext, NlDispatchResult};

/// チャットメッセージのロール
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChatRole {
    User,
    Assistant,
}

/// チャット履歴の1エントリ
#[derive(Debug, Clone)]
pub struct ChatMessage {
    pub role: ChatRole,
    pub content: String,
}

/// "__local:" プレフィックスから解釈されるViewer側ローカルコマンド
#[derive(Debug, Clone, PartialEq)]
pub enum LocalCommand {
    SwitchMode2D,
    SwitchMode3D,
    Search(String),
}

impl LocalCommand {
    pub fn parse(response: &str) -> Option<Self> {
        let s = response.strip_prefix("__local:")?;
        if s == "switch_2d" {
            return Some(Self::SwitchMode2D);
        }
        if s == "switch_3d" {
            return Some(Self::SwitchMode3D);
        }
        if let Some(term) = s.strip_prefix("search:") {
            return Some(Self::Search(term.to_string()));
        }
        None
    }
}

/// NLチャットパネルの状態
pub struct NlChatPanel {
    pub history: Vec<ChatMessage>,
    pub input: String,
}

impl NlChatPanel {
    pub fn new() -> Self {
        Self {
            history: vec![ChatMessage {
                role: ChatRole::Assistant,
                content: "こんにちは！構造マップについて何でも質問してください。\n例: 「循環依存を見せて」「プレビューして」「適用して」「元に戻して」".to_string(),
            }],
            input: String::new(),
        }
    }

    /// パネルをレンダリングし、ローカルコマンドがあれば返す
    pub fn render(
        &mut self,
        ui: &mut Ui,
        selected_node: Option<&str>,
        root: &Path,
    ) -> Option<LocalCommand> {
        let mut local_cmd = None;

        // チャット履歴エリア
        let available = ui.available_height() - 56.0;
        ScrollArea::vertical()
            .id_salt("nl_chat_scroll")
            .max_height(available.max(100.0))
            .stick_to_bottom(true)
            .show(ui, |ui| {
                for msg in &self.history {
                    render_message(ui, msg);
                    ui.add_space(4.0);
                }
            });

        ui.separator();

        // 入力エリア
        ui.horizontal(|ui| {
            let input_width = ui.available_width() - 40.0;
            let response = ui.add(
                egui::TextEdit::singleline(&mut self.input)
                    .desired_width(input_width)
                    .hint_text("メッセージを入力…")
                    .font(TextStyle::Body),
            );

            let submitted = (response.lost_focus()
                && ui.input(|i| i.key_pressed(egui::Key::Enter)))
                || ui.button("→").clicked();

            if submitted && !self.input.is_empty() {
                let prompt = std::mem::take(&mut self.input);
                self.history.push(ChatMessage {
                    role: ChatRole::User,
                    content: prompt.clone(),
                });

                let result = dispatch_nl_direct(&prompt, selected_node, root);
                if let Some(cmd) = LocalCommand::parse(&result.response) {
                    local_cmd = Some(cmd);
                } else {
                    self.history.push(ChatMessage {
                        role: ChatRole::Assistant,
                        content: result.response,
                    });
                }

                response.request_focus();
            }
        });

        local_cmd
    }
}

fn render_message(ui: &mut Ui, msg: &ChatMessage) {
    let text_color = match msg.role {
        ChatRole::User => Color32::from_rgb(30, 60, 120),
        ChatRole::Assistant => Color32::from_rgb(40, 40, 40),
    };
    let prefix = match msg.role {
        ChatRole::User => "You: ",
        ChatRole::Assistant => "▸ ",
    };
    ui.label(
        RichText::new(format!("{prefix}{}", msg.content))
            .font(FontId::proportional(13.0))
            .color(text_color),
    );
}

/// design_cli の dispatch_nl を直接呼ぶ（viewer_gui は design_cli に直接依存するため）
fn dispatch_nl_direct(prompt: &str, selected_node: Option<&str>, root: &Path) -> NlDispatchResult {
    let ctx = NlContext {
        prompt: prompt.to_string(),
        selected_node: selected_node.map(str::to_string),
        root: root.to_path_buf(),
    };
    design_cli::viewer::nl_dispatch::dispatch_nl(&ctx)
}
