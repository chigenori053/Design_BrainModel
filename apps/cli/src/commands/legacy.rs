use crate::command::{CommandHandler, CommandPlugin, CommandRegistry, Output, SubCommandHandler};

pub struct LegacyPlugin;

impl CommandPlugin for LegacyPlugin {
    fn register(&self, registry: &mut CommandRegistry) {
        let mut phase1 = CommandHandler::new("phase1");
        phase1.register_subcommand(SubCommandHandler::new("run", |args, _| {
            let forwarded = args
                .iter()
                .map(std::ffi::OsString::from)
                .collect::<Vec<_>>();
            crate::cli::commands::phase1::run(forwarded)
                .map(|_| Output::text("Phase1 completed"))
                .map_err(crate::command::CommandError::ExecutionError)
        }));
        registry.register(phase1);

        let mut phase_analyze = CommandHandler::new("phase-analyze");
        phase_analyze.register_subcommand(SubCommandHandler::new("run", |args, _| {
            let mut forwarded = vec![
                std::ffi::OsString::from("design_cli"),
                std::ffi::OsString::from("phase-analyze"),
            ];
            forwarded.extend(args.iter().map(std::ffi::OsString::from));
            crate::design_main::run_with_args(forwarded)
                .map(|_| Output::text("PhaseAnalyze completed"))
                .map_err(crate::command::CommandError::ExecutionError)
        }));
        registry.register(phase_analyze);
    }
}
