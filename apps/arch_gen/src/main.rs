use clap::{Parser, Subcommand};

mod commands;
mod input_bridge;
mod output;
mod store;
mod template;

#[derive(Parser, Debug)]
#[command(
    name = "arch_gen",
    about = "Architecture Generative AI\nDesign_BrainModel Core — アーキテクチャ生成 AI ツール\n\nコマンド例:\n  arch_gen /generate \"要件テキスト\"\n  arch_gen /interactive\n  arch_gen /scan ./src",
    disable_version_flag = true
)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// 要件テキストからアーキテクチャ候補を生成しコードを出力する
    #[command(name = "/generate", visible_alias = "generate")]
    Generate {
        /// 要件テキスト（省略時は対話開始。@ファイルパス 形式、または "-" で stdin から読み込み）
        requirement: Option<String>,

        /// 出力する候補数
        #[arg(short = 'n', long, default_value_t = 3, env = "ARCH_GEN_CANDIDATES")]
        candidates: usize,

        /// 出力ディレクトリ
        #[arg(short, long, default_value = "./arch_out", env = "ARCH_GEN_OUTPUT")]
        output: String,

        /// 出力形式: text | json | mermaid | markdown | plantuml
        #[arg(short, long, default_value = "text", env = "ARCH_GEN_FORMAT")]
        format: String,

        /// ビームサーチ幅
        #[arg(long = "beam-width", default_value_t = 10, env = "ARCH_GEN_BEAM_WIDTH")]
        beam_width: usize,

        /// サーチ深度
        #[arg(long = "max-depth", default_value_t = 5)]
        max_depth: usize,

        /// コード生成を明示的にスキップする（互換オプション）
        #[arg(long)]
        no_code: bool,

        /// 候補確定後のソースファイル生成を有効にする
        #[arg(long)]
        write_files: bool,

        /// 詳細ログを出力
        #[arg(long)]
        verbose: bool,

        /// 書き出し戦略: new | merge | overwrite | dry-run
        #[arg(long = "output-strategy", default_value = "new")]
        output_strategy: String,

        /// 出力レイアウト: flat | module
        #[arg(long = "output-layout", default_value = "flat")]
        output_layout: String,

        /// 生成ファイルを git add する
        #[arg(long)]
        git_add: bool,

        /// 生成後に OS デフォルトアプリで出力を開く
        #[arg(long)]
        open: bool,

        /// テンプレート入力ステップをスキップする（非対話・CI向け）
        #[arg(long)]
        no_template: bool,
    },

    /// 保存済み設計ファイルを評価してスコアを表示する
    #[command(name = "/evaluate", visible_alias = "evaluate")]
    Evaluate {
        /// 設計ファイルパス（JSON）
        design_file: String,
    },

    /// 設計ファイルを指定フォーマットで出力する
    #[command(name = "/export", visible_alias = "export")]
    Export {
        /// 設計ファイルパス（JSON）
        design_file: String,

        /// 出力形式: json | mermaid | markdown | plantuml | text
        #[arg(short, long, env = "ARCH_GEN_FORMAT")]
        format: String,

        /// 出力先ファイルパス（省略時は stdout）
        #[arg(short, long)]
        output: Option<String>,

        /// 生成後に OS デフォルトアプリで開く
        #[arg(long)]
        open: bool,
    },

    /// 設計の説明レポートを生成する
    #[command(name = "/explain", visible_alias = "explain")]
    Explain {
        /// 設計ファイルパス（JSON）
        design_file: String,
    },

    /// 知識量とWebSearch配線状態を監査する
    #[command(name = "/knowledge-audit", visible_alias = "knowledge-audit")]
    KnowledgeAudit {
        /// 出力形式: text | json
        #[arg(short, long, default_value = "text")]
        format: String,
    },

    /// 必要に応じてWeb検索で補助知識を取得する
    #[command(name = "/web-search", visible_alias = "web-search")]
    WebSearch {
        /// 検索クエリ
        query: String,

        /// 出力件数
        #[arg(short = 'n', long, default_value_t = 5)]
        limit: usize,

        /// 出力形式: text | json
        #[arg(short, long, default_value = "text")]
        format: String,
    },

    /// 追加条件で設計を再探索する
    #[command(name = "/refine", visible_alias = "refine")]
    Refine {
        /// 設計ファイルパス（JSON）
        design_file: String,

        /// 追加要件テキスト
        additional_requirement: String,
    },

    /// ローカルソースコードを読み込んでアーキテクチャを逆解析する
    #[command(name = "/scan", visible_alias = "scan")]
    Scan {
        /// スキャン対象ディレクトリ
        dir: String,

        /// 出力形式: text | mermaid | markdown | json | plantuml
        #[arg(short, long, default_value = "text", env = "ARCH_GEN_FORMAT")]
        format: String,

        /// 出力先ファイルパス（省略時は stdout）
        #[arg(short, long)]
        output: Option<String>,

        /// スキャン深度
        #[arg(long, default_value_t = 3)]
        depth: usize,

        /// 対象ファイルパターン（glob）
        #[arg(long, default_value = "**/*.rs")]
        include: String,

        /// 詳細ログを出力
        #[arg(long)]
        verbose: bool,
    },

    /// YAML spec から Architecture Search Engine を実行する
    #[command(name = "/search", visible_alias = "search")]
    Search {
        /// 探索仕様 YAML
        spec_file: String,

        /// 出力ディレクトリ
        #[arg(short, long, default_value = "./architectures")]
        output: String,

        /// ビーム幅
        #[arg(long = "beam-width", default_value_t = 8)]
        beam_width: usize,

        /// 探索深度
        #[arg(long = "max-depth", default_value_t = 6)]
        max_depth: usize,

        /// 評価する最大候補数
        #[arg(long = "max-candidates", default_value_t = 1024)]
        max_candidates: usize,

        /// Pareto frontier の最大件数
        #[arg(long = "pareto-limit", default_value_t = 10)]
        pareto_limit: usize,

        /// タイムアウト (ms)
        #[arg(long = "timeout-ms", default_value_t = 10000)]
        timeout_ms: u64,
    },

    /// 対話型設計精緻化モード
    #[command(name = "/interactive", visible_alias = "interactive")]
    Interactive {
        /// 既存設計ファイルから開始
        #[arg(long)]
        from: Option<String>,
    },

    /// `interactive` のエイリアス
    #[command(name = "/i", visible_alias = "i")]
    InteractiveAlias {
        #[arg(long)]
        from: Option<String>,
    },

    /// 名前付きで設計を保存・一覧・読み込みするストア管理
    #[command(name = "/saves", visible_alias = "saves")]
    Saves {
        /// 保存名（省略時は一覧表示）
        name: Option<String>,

        /// 読み込むエントリ名
        #[arg(long)]
        load: Option<String>,

        /// 削除するエントリ名
        #[arg(long)]
        delete: Option<String>,

        /// 読み込み先の設計ファイルパス（--load と組み合わせて使用）
        #[arg(short, long)]
        output: Option<String>,
    },
}

fn main() {
    let cli = Cli::parse();
    // サブコマンドなし → インタラクティブモードへ
    let result = match cli.command {
        Some(cmd) => dispatch(cmd),
        None => commands::interactive::run(commands::interactive::InteractiveArgs { from: None }),
    };

    if let Err(e) = result {
        eprintln!("error: {e}");
        std::process::exit(1);
    }
}

fn dispatch(cmd: Commands) -> Result<(), String> {
    match cmd {
        Commands::Generate {
            requirement,
            candidates,
            output,
            format,
            beam_width,
            max_depth,
            no_code,
            write_files,
            verbose,
            output_strategy,
            output_layout,
            git_add,
            open,
            no_template,
        } => {
            let Some(requirement) = requirement else {
                return commands::interactive::run(commands::interactive::InteractiveArgs {
                    from: None,
                });
            };

            // "-" → stdin から要件テキストを読み込む
            let requirement = if requirement == "-" {
                read_stdin()?
            } else {
                requirement
            };

            let result = commands::generate::run(commands::generate::GenerateArgs {
                requirement,
                candidates,
                output_dir: output.clone(),
                format: format.clone(),
                beam_width,
                max_depth,
                no_code,
                write_files,
                verbose,
                output_strategy,
                output_layout,
                no_template,
            });

            if result.is_ok() {
                if git_add {
                    git_add_dir(&output)?;
                }
                if open {
                    open_path(&output);
                }
            }
            result
        }

        Commands::Evaluate { design_file } => commands::evaluate::run(&design_file),

        Commands::Export {
            design_file,
            format,
            output,
            open,
        } => {
            let result = commands::export::run(&design_file, &format, output.as_deref());
            if result.is_ok() {
                if open {
                    if let Some(ref path) = output {
                        open_path(path);
                    }
                }
            }
            result
        }

        Commands::Explain { design_file } => commands::explain::run(&design_file),

        Commands::KnowledgeAudit { format } => commands::knowledge_audit::run(&format),

        Commands::WebSearch {
            query,
            limit,
            format,
        } => commands::web_search::run(&query, limit, &format),

        Commands::Refine {
            design_file,
            additional_requirement,
        } => {
            // additional_requirement も "-" で stdin 対応
            let additional = if additional_requirement == "-" {
                read_stdin()?
            } else {
                additional_requirement
            };
            commands::refine::run(&design_file, &additional)
        }

        Commands::Scan {
            dir,
            format,
            output,
            depth,
            include,
            verbose,
        } => commands::scan::run(commands::scan::ScanArgs {
            dir,
            format,
            output,
            depth,
            include,
            verbose,
        }),

        Commands::Search {
            spec_file,
            output,
            beam_width,
            max_depth,
            max_candidates,
            pareto_limit,
            timeout_ms,
        } => commands::search::run(commands::search::SearchArgs {
            spec_file,
            output_dir: output,
            beam_width,
            max_depth,
            max_candidates,
            pareto_limit,
            timeout_ms,
        }),

        Commands::Interactive { from } | Commands::InteractiveAlias { from } => {
            commands::interactive::run(commands::interactive::InteractiveArgs { from })
        }

        Commands::Saves { name, load, delete, output } => {
            run_saves(name, load, delete, output)
        }
    }
}

fn run_saves(
    name: Option<String>,
    load: Option<String>,
    delete: Option<String>,
    output: Option<String>,
) -> Result<(), String> {
    use store::{DesignStore, format_store_list};

    let store = DesignStore::new();

    if let Some(del_name) = delete {
        store.delete(&del_name)?;
        println!("Deleted '{del_name}' from store.");
        return Ok(());
    }

    if let Some(load_name) = load {
        let design = store.load(&load_name)?;
        let out_path = output.as_deref().unwrap_or("design_loaded.json");
        crate::input_bridge::save_design_file(&design, std::path::Path::new(out_path))?;
        println!("Loaded '{load_name}' → {out_path}");
        return Ok(());
    }

    if let Some(save_name) = name {
        // design.json をストアに保存
        let src = std::path::Path::new("./arch_out/design.json");
        let design = crate::input_bridge::load_design_file(src)
            .map_err(|_| "design.json not found. Run `generate` first.".to_string())?;
        let path = store.save(&save_name, &design)?;
        println!("Saved as '{}' → {}", save_name, path.display());
        return Ok(());
    }

    // 省略時は一覧表示
    let entries = store.list()?;
    println!("{}", format_store_list(&entries));
    Ok(())
}

// ─── 外部ツール連携ヘルパー ────────────────────────────────────────────────────

/// stdin から改行を含むテキストをすべて読み込む。
fn read_stdin() -> Result<String, String> {
    use std::io::Read;
    let mut buf = String::new();
    std::io::stdin()
        .read_to_string(&mut buf)
        .map_err(|e| format!("failed to read stdin: {e}"))?;
    let trimmed = buf.trim().to_string();
    if trimmed.is_empty() {
        return Err("stdin was empty".to_string());
    }
    Ok(trimmed)
}

/// 指定ディレクトリ配下のファイルを `git add` する。
fn git_add_dir(dir: &str) -> Result<(), String> {
    let status = std::process::Command::new("git")
        .args(["add", dir])
        .status()
        .map_err(|e| format!("failed to run git: {e}"))?;
    if !status.success() {
        return Err(format!(
            "git add '{dir}' failed with exit code {:?}",
            status.code()
        ));
    }
    eprintln!("[arch_gen] git add {dir}");
    Ok(())
}

/// OS デフォルトアプリでパスを開く（macOS: open, Linux: xdg-open）。
fn open_path(path: &str) {
    #[cfg(target_os = "macos")]
    let cmd = "open";
    #[cfg(target_os = "linux")]
    let cmd = "xdg-open";
    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    let cmd = "start";

    if let Err(e) = std::process::Command::new(cmd).arg(path).spawn() {
        eprintln!("[arch_gen] warning: could not open '{path}': {e}");
    }
}
