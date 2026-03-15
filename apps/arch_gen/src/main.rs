use clap::{Parser, Subcommand};

mod commands;
mod input_bridge;
mod output;

#[derive(Parser, Debug)]
#[command(
    name = "arch-gen",
    about = "Architecture Generative AI — Design_BrainModel Core frontend",
    disable_version_flag = true
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// 要件テキストからアーキテクチャ候補を生成しコードを出力する
    Generate {
        /// 要件テキスト（@ファイルパス 形式、または "-" で stdin から読み込み）
        requirement: String,

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

        /// コード生成をスキップ（高速化）
        #[arg(long)]
        no_code: bool,

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
    },

    /// 保存済み設計ファイルを評価してスコアを表示する
    Evaluate {
        /// 設計ファイルパス（JSON）
        design_file: String,
    },

    /// 設計ファイルを指定フォーマットで出力する
    Export {
        /// 設計ファイルパス（JSON）
        design_file: String,

        /// 出力形式: json | mermaid | markdown | plantuml | code | text
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
    Explain {
        /// 設計ファイルパス（JSON）
        design_file: String,
    },

    /// 追加条件で設計を再探索する
    Refine {
        /// 設計ファイルパス（JSON）
        design_file: String,

        /// 追加要件テキスト
        additional_requirement: String,
    },

    /// ローカルソースコードを読み込んでアーキテクチャを逆解析する
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

    /// 対話型設計精緻化モード
    Interactive {
        /// 既存設計ファイルから開始
        #[arg(long)]
        from: Option<String>,
    },

    /// `interactive` のエイリアス
    #[command(name = "i")]
    InteractiveAlias {
        #[arg(long)]
        from: Option<String>,
    },
}

fn main() {
    let cli = Cli::parse();
    let result = dispatch(cli.command);

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
            verbose,
            output_strategy,
            output_layout,
            git_add,
            open,
        } => {
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
                verbose,
                output_strategy,
                output_layout,
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

        Commands::Export { design_file, format, output, open } => {
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

        Commands::Refine { design_file, additional_requirement } => {
            // additional_requirement も "-" で stdin 対応
            let additional = if additional_requirement == "-" {
                read_stdin()?
            } else {
                additional_requirement
            };
            commands::refine::run(&design_file, &additional)
        }

        Commands::Scan { dir, format, output, depth, include, verbose } => {
            commands::scan::run(commands::scan::ScanArgs {
                dir,
                format,
                output,
                depth,
                include,
                verbose,
            })
        }

        Commands::Interactive { from } | Commands::InteractiveAlias { from } => {
            commands::interactive::run(commands::interactive::InteractiveArgs { from })
        }
    }
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
        return Err(format!("git add '{dir}' failed with exit code {:?}", status.code()));
    }
    eprintln!("[arch-gen] git add {dir}");
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
        eprintln!("[arch-gen] warning: could not open '{path}': {e}");
    }
}
