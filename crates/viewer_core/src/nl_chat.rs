use eframe::egui::{self, Color32, FontId, RichText, ScrollArea, TextStyle, Ui};

use crate::model::DispatchNl;

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
    pub pending: bool,
}

impl NlChatPanel {
    pub fn new() -> Self {
        Self {
            history: vec![ChatMessage {
                role: ChatRole::Assistant,
                content: "Hello! Ask anything about the structure map.\nExamples: show cycles / preview refactor / apply / undo".to_string(),
            }],
            input: String::new(),
            pending: false,
        }
    }

    /// パネルをレンダリングし、送信が発生した場合は `Some((prompt, needs_ir_reload))` を返す
    /// ローカルコマンドは `Option<LocalCommand>` として別途返す
    pub fn render(
        &mut self,
        ui: &mut Ui,
        selected_node: Option<&str>,
        dispatch: &DispatchNl,
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
                if self.pending {
                    ui.label(
                        RichText::new("▪▪▪")
                            .color(Color32::from_rgb(120, 120, 180))
                            .font(FontId::proportional(13.0)),
                    );
                }
            });

        ui.separator();

        // 入力エリア
        ui.horizontal(|ui| {
            let input_width = ui.available_width() - 40.0;
            let response = ui.add(
                egui::TextEdit::singleline(&mut self.input)
                    .desired_width(input_width)
                    .hint_text("Enter message...")
                    .font(TextStyle::Body),
            );

            let submitted = (response.lost_focus()
                && ui.input(|i| i.key_pressed(egui::Key::Enter)))
                || ui.button("→").clicked();

            if submitted && !self.input.is_empty() && !self.pending {
                let prompt = std::mem::take(&mut self.input);
                self.history.push(ChatMessage {
                    role: ChatRole::User,
                    content: prompt.clone(),
                });

                let result_json = dispatch(&prompt, selected_node);
                match result_json {
                    Ok(json) => {
                        let (response_text, ir_updated) = parse_result(&json);
                        let _ = ir_updated; // ViewerApp側がIrTrackerで自動検知するため不要
                        if let Some(cmd) = LocalCommand::parse(&response_text) {
                            local_cmd = Some(cmd);
                            // ローカルコマンドは履歴に表示しない
                        } else {
                            self.history.push(ChatMessage {
                                role: ChatRole::Assistant,
                                content: response_text,
                            });
                        }
                    }
                    Err(e) => {
                        self.history.push(ChatMessage {
                            role: ChatRole::Assistant,
                            content: format!("エラー: {e}"),
                        });
                    }
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

/// DispatchNlが返すJSONを解析して (response_text, ir_updated) を取り出す
fn parse_result(json: &str) -> (String, bool) {
    if let Ok(val) = serde_json::from_str::<serde_json::Value>(json) {
        let text = val
            .get("response")
            .and_then(|v| v.as_str())
            .unwrap_or(json)
            .to_string();
        let ir_updated = val
            .get("ir_updated")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        return (text, ir_updated);
    }
    (json.to_string(), false)
}
