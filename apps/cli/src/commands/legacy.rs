use crate::command::{CommandHandler, CommandPlugin, CommandRegistry, Output, SubCommandHandler};

pub struct LegacyPlugin;

impl CommandPlugin for LegacyPlugin {
    fn register(&self, registry: &mut CommandRegistry) {
        let mut phase1 = CommandHandler::new("phase1");
        phase1.register_subcommand(SubCommandHandler::new("run", |args, _| {
            let forwarded = args
                .iter()
                .map(|s| std::ffi::OsString::from(s))
                .collect::<Vec<_>>();
            crate::cli::commands::phase1::run(forwarded)
                .map(|_| Output::text("Phase1 completed"))
                .map_err(|e| crate::command::CommandError::ExecutionError(e))
        }));
        registry.register(phase1);

        let mut phase_analyze = CommandHandler::new("phase-analyze");
        phase_analyze.register_subcommand(SubCommandHandler::new("run", |args, _| {
            let mut forwarded = vec![
                std::ffi::OsString::from("design_cli"),
                std::ffi::OsString::from("phase-analyze"),
            ];
            forwarded.extend(args.iter().map(|s| std::ffi::OsString::from(s)));
            crate::design_main::run_with_args(forwarded)
                .map(|_| Output::text("PhaseAnalyze completed"))
                .map_err(|e| crate::command::CommandError::ExecutionError(e))
        }));
        registry.register(phase_analyze);
    }
}
