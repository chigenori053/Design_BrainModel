use clap::Parser;
use design_cli::core::{CoreExecutor, CoreRequest, RuntimeCoreBridge};
use design_cli::runtime::bootstrap::start_runtime_tui;

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

    if let Some(help) = subcommand_help(&cli.input) {
        println!("{help}");
        return;
    }

    if raw_input == "repl" {
        if let Err(err) = start_runtime_tui() {
            eprintln!("{err}");
            std::process::exit(1);
        }
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

fn subcommand_help(input: &[String]) -> Option<&'static str> {
    let [command, flag] = input else {
        return None;
    };
    if flag != "--help" && flag != "-h" {
        return None;
    }
    match command.as_str() {
        "coding" => Some(CODING_HELP),
        "structure" => Some(STRUCTURE_HELP),
        "repl" => Some(REPL_HELP),
        _ => None,
    }
}

const CODING_HELP: &str = "\
Usage: design_cli coding [--check] [--apply]

Options:
  --check   Validate generated coding changes.
";

const STRUCTURE_HELP: &str = "\
Usage: design_cli structure <view|edit|diff>

Commands:
  view      Render structure view.
";

const REPL_HELP: &str = "\
Usage: design_cli repl [--json]

Start the deterministic runtime host loop.

Options:
  --json    Reserved for structured runtime output.
";
// DBM clarification execution guarantee
