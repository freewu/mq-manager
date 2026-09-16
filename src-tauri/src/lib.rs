//! MQ Manager backend.
//!
//! Layering (see `mq/mod.rs` for the picture):
//!
//! * `mq::drivers::*`   — one module per broker, all sharing the neutral model
//! * `mq::provider`     — the traits a driver implements
//! * `commands::*`      — thin Tauri IPC adapters over the neutral traits
//! * `state`/`config`   — live connections, background jobs, persisted workspace
//!
//! Nothing above `mq::drivers` mentions Kafka, RocketMQ, RabbitMQ or MQTT.

mod commands;
mod config;
mod error;
mod events;
mod mq;
mod state;

use tauri::Manager;

pub fn run() {
    init_tracing();
    // Both `lapin` and `reqwest` are built without a bundled rustls crypto
    // provider, so exactly one must be installed process wide. `ring` is used
    // because it needs nothing but a C compiler (no CMake / NASM).
    install_crypto_provider();

    let registry = mq::drivers::registry_from_builtin();

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .setup(move |app| {
            let workspace = config::WorkspaceStore::load(app.handle())?;
            app.manage(state::AppState::new(registry, workspace));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            // app / settings
            commands::app::app_info,
            commands::app::get_settings,
            commands::app::save_settings,
            // providers & connections
            commands::connections::list_providers,
            commands::connections::list_connections,
            commands::connections::connection_statuses,
            commands::connections::connection_capabilities,
            commands::connections::save_connection,
            commands::connections::test_connection,
            commands::connections::connect,
            commands::connections::disconnect,
            commands::connections::remove_connection,
            // cluster
            commands::cluster::cluster_info,
            commands::cluster::list_nodes,
            commands::cluster::ping,
            // topics
            commands::topics::list_topics,
            commands::topics::get_topic,
            commands::topics::get_topic_configs,
            commands::topics::create_topic,
            commands::topics::update_topic,
            commands::topics::delete_topic,
            commands::topics::purge_topic,
            // messages
            commands::messages::produce_message,
            commands::messages::browse_messages,
            commands::messages::start_stream,
            commands::messages::stop_stream,
            commands::messages::list_jobs,
            // consumer groups
            commands::groups::list_groups,
            commands::groups::get_group,
            commands::groups::delete_group,
            commands::groups::reset_group_offsets,
        ])
        .build(tauri::generate_context!())
        .expect("failed to build the MQ Manager application")
        .run(|app, event| {
            if let tauri::RunEvent::Exit = event {
                let state = app.state::<state::AppState>();
                tauri::async_runtime::block_on(commands::connections::disconnect_all(&state));
            }
        });
}

fn init_tracing() {
    use tracing_subscriber::EnvFilter;

    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| {
        EnvFilter::new(if cfg!(debug_assertions) {
            "info,mq_manager_lib=debug"
        } else {
            "warn,mq_manager_lib=info"
        })
    });

    let _ = tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(false)
        .try_init();
}

/// Install the rustls crypto provider exactly once.
///
/// `lapin`/`reqwest` are compiled without a bundled provider; the first caller
/// wins and later calls return an error which is safe to ignore.
pub(crate) fn install_crypto_provider() {
    if rustls::crypto::ring::default_provider()
        .install_default()
        .is_err()
    {
        tracing::debug!("a rustls crypto provider was already installed");
    }
}
