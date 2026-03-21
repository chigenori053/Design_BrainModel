use super::registry::CommandRegistry;

/// CommandPlugin トレイト
///
/// Command群をひとまとめにして Registry に登録するための単位。
/// 機能ごとに Plugin を分割することで、既存コードを変更せずに Command を追加できる。
///
/// # 例
/// ```ignore
/// pub struct GeneratePlugin;
///
/// impl CommandPlugin for GeneratePlugin {
///     fn register(&self, registry: &mut CommandRegistry) {
///         let mut cmd = CommandHandler::new("generate");
///         cmd.register_subcommand(spec::handler());
///         registry.register(cmd);
///     }
/// }
/// ```
pub trait CommandPlugin {
    fn register(&self, registry: &mut CommandRegistry);
}
