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
        /// 要件テキスト（@ファイルパス 形式でファイル読み込みも可）
        requirement: String,

        /// 出力する候補数
        #[arg(short = 'n', long, default_value_t = 3)]
        candidates: usize,

        /// 出力ディレクトリ
        #[arg(short, long, default_value = "./arch_out")]
        output: String,

        /// 出力形式: text | json | mermaid | markdown
        #[arg(short, long, default_value = "text")]
        format: String,

        /// ビームサーチ幅
        #[arg(long = "beam-width", default_value_t = 10)]
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

        /// 出力形式: json | mermaid | markdown | plantuml | code
        #[arg(short, long)]
        format: String,

        /// 出力先ファイルパス（省略時は stdout）
        #[arg(short, long)]
        output: Option<String>,
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
}

fn main() {
    let cli = Cli::parse();
    let result = match cli.command {
        Commands::Generate {
            requirement,
            candidates,
            output,
            format,
            beam_width,
            max_depth,
            no_code,
            verbose,
        } => commands::generate::run(commands::generate::GenerateArgs {
            requirement,
            candidates,
            output_dir: output,
            format,
            beam_width,
            max_depth,
            no_code,
            verbose,
        }),
        Commands::Evaluate { design_file } => commands::evaluate::run(&design_file),
        Commands::Export {
            design_file,
            format,
            output,
        } => commands::export::run(&design_file, &format, output.as_deref()),
        Commands::Explain { design_file } => commands::explain::run(&design_file),
        Commands::Refine {
            design_file,
            additional_requirement,
        } => commands::refine::run(&design_file, &additional_requirement),
    };

    if let Err(e) = result {
        eprintln!("error: {e}");
        std::process::exit(1);
    }
}
