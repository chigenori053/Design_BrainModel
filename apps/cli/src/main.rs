use clap::Parser;
use design_cli::core::{CoreExecutor, CoreRequest, RuntimeCoreBridge};

const VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Parser, Debug)]
#[command(
    name = "design_cli",
    version = VERSION,
    about = "AI-native architecture analysis, safe refactoring, and structure visualization CLI",
    disable_help_subcommand = true,
    allow_external_subcommands = true,
)]
struct Cli {
    #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
    input: Vec<String>,
}

fn main() {
    ctrlc::set_handler(move || {
        std::process::exit(0);
    })
    .expect("Error setting Ctrl-C handler");

    let cli = Cli::parse();

    // §5.1 collect raw input
    let raw_input = if cli.input.is_empty() {
        String::new()
    } else {
        cli.input.join(" ")
    };

    if raw_input.is_empty() || raw_input == "help" {
        println!("{}", ONBOARDING_HELP);
        return;
    }

    // §5.2 Core connection
    let core = RuntimeCoreBridge::with_defaults();

    let request = CoreRequest::new(raw_input);

    let response = core.execute(request);

    // Render output
    for event in response.events {
        match event {
            design_cli::core::CoreEvent::Result { message } => println!("{}", message),
            design_cli::core::CoreEvent::Error { message } => eprintln!("[ERROR] {}", message),
            _ => {}
        }
    }

    if response.status == design_cli::core::ExecutionStatus::Failed {
        std::process::exit(1);
    }
}

const ONBOARDING_HELP: &str = "\
AI-native architecture analysis, safe refactoring, and structure visualization CLI

Usage: design_cli [INPUT]...

Examples:
  design_cli \"/analyze .\"
  design_cli \"/structure view .\"
  design_cli \"このプロジェクトを解析して\"

Use /help to see all available slash commands.
";
