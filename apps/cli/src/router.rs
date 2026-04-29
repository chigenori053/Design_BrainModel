/// Phase0 Patch: 入力ルーター（構造化Command対応）
///
/// Phase1 CommandRegistry への直接接続を可能にするため、
/// Command を name / subcommand / args に構造化する。
///
/// ルーティングルール：
/// - `/コマンド [サブコマンド] [引数...]` → Command ルート
/// - それ以外 → Agent ルート（将来 /plan に接続）
///
/// ルーティング結果
///
/// `Route::Command` は Phase1 で CommandRegistry.execute() に直接接続される。
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Route {
    /// 明示的なコマンド（構造化済み）
    ///
    /// 例: `/generate spec cli` → name="generate", subcommand=Some("spec"), args=["cli"]
    Command {
        /// コマンド名（`/` を除いたもの）
        name: String,
        /// サブコマンド（Phase1で使用）
        subcommand: Option<String>,
        /// 追加引数
        args: Vec<String>,
    },
    /// エージェントへの自然言語入力
    Agent(String),
}

/// 入力文字列をルーティングする
///
/// `/` で始まる場合は Command に構造化、それ以外は Agent に振り分ける。
///
/// # パース仕様
/// ```text
/// /generate spec cli
///  ^^^^^^^^ ^^^^ ^^^
///  name     sub  args[0]
/// ```
pub fn route(input: &str) -> Route {
    if !input.starts_with('/') {
        return Route::Agent(input.to_string());
    }

    let parts: Vec<&str> = input.split_whitespace().collect();
    let name = parts[0].trim_start_matches('/').to_string();
    let subcommand = parts.get(1).map(|s| s.to_string());
    let args = parts
        .get(2..)
        .unwrap_or(&[])
        .iter()
        .map(|s| s.to_string())
        .collect();

    Route::Command {
        name,
        subcommand,
        args,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slash_prefix_routes_to_command() {
        let r = route("/exit");
        assert_eq!(
            r,
            Route::Command {
                name: "exit".to_string(),
                subcommand: None,
                args: vec![],
            }
        );
    }

    #[test]
    fn no_slash_routes_to_agent() {
        let r = route("APIのレイテンシを改善したい");
        assert_eq!(r, Route::Agent("APIのレイテンシを改善したい".to_string()));
    }

    #[test]
    fn empty_slash_routes_to_command_with_empty_name() {
        // "/" 単体もコマンドとして扱う（ハンドラ側でエラー）
        let r = route("/");
        assert!(matches!(r, Route::Command { .. }));
    }

    #[test]
    fn plain_text_routes_to_agent() {
        assert!(matches!(route("hello"), Route::Agent(_)));
        assert!(matches!(route("  spaces  "), Route::Agent(_)));
    }

    #[test]
    fn command_with_subcommand_is_parsed() {
        let r = route("/generate spec");
        assert_eq!(
            r,
            Route::Command {
                name: "generate".to_string(),
                subcommand: Some("spec".to_string()),
                args: vec![],
            }
        );
    }

    #[test]
    fn command_with_subcommand_and_args_is_parsed() {
        // /generate spec cli → name=generate, subcommand=spec, args=["cli"]
        let r = route("/generate spec cli");
        assert_eq!(
            r,
            Route::Command {
                name: "generate".to_string(),
                subcommand: Some("spec".to_string()),
                args: vec!["cli".to_string()],
            }
        );
    }

    #[test]
    fn command_with_multiple_args() {
        // /run plan --dry-run → name=run, subcommand=plan, args=["--dry-run"]
        let r = route("/run plan --dry-run");
        assert_eq!(
            r,
            Route::Command {
                name: "run".to_string(),
                subcommand: Some("plan".to_string()),
                args: vec!["--dry-run".to_string()],
            }
        );
    }

    #[test]
    fn command_name_is_stripped_of_slash() {
        let Route::Command { name, .. } = route("/help") else {
            panic!("expected Command");
        };
        assert_eq!(name, "help");
    }

    #[test]
    fn command_without_subcommand_has_none() {
        let Route::Command { subcommand, .. } = route("/status") else {
            panic!("expected Command");
        };
        assert_eq!(subcommand, None);
    }
}
