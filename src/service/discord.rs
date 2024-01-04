use super::{PinnedBoxedFutureResult, Priority, Service, ServiceInfo, ServiceManager};
use log::info;
use serenity::{
    all::{GatewayIntents, Ready},
    async_trait,
    client::{self, Cache, Context},
    framework::{standard::Configuration, StandardFramework},
    gateway::{ShardManager, VoiceGatewayManager},
    http::Http,
    prelude::TypeMap,
    Client, Error,
};
use std::{sync::Arc, time::Duration};
use tokio::{
    sync::{Mutex, Notify, RwLock},
    task::JoinHandle,
    time::{sleep, timeout},
};

pub struct DiscordService {
    info: ServiceInfo,
    discord_token: String,
    connection_timeout: Duration,
    pub client: Arc<Mutex<Option<Ready>>>,
    client_handle: Option<JoinHandle<Result<(), Error>>>,
    pub cache: Option<Arc<Cache>>,
    pub data: Option<Arc<RwLock<TypeMap>>>,
    pub http: Option<Arc<Http>>,
    pub shard_manager: Option<Arc<ShardManager>>,
    pub voice_manager: Option<Arc<dyn VoiceGatewayManager>>,
    pub ws_url: Option<Arc<Mutex<String>>>,
}

impl DiscordService {
    pub fn new(discord_token: &str, connection_timeout: Duration) -> Self {
        Self {
            info: ServiceInfo::new("lum_builtin_discord", "Discord", Priority::Essential),
            discord_token: "discord_token".to_string(),
            connection_timeout,
            client: Arc::new(Mutex::new(None)),
            client_handle: None,
            cache: None,
            data: None,
            http: None,
            shard_manager: None,
            voice_manager: None,
            ws_url: None,
        }
    }
}

impl Service for DiscordService {
    fn info(&self) -> &ServiceInfo {
        &self.info
    }

    fn start(&mut self, service_manager: &ServiceManager) -> PinnedBoxedFutureResult<'_, ()> {
        Box::pin(async move {
            let framework = StandardFramework::new();
            framework.configure(Configuration::new().prefix("!"));

            let client_ready_notify = Arc::new(Notify::new());

            let mut client = Client::builder(self.discord_token.as_str(), GatewayIntents::all())
                .framework(framework)
                .event_handler(EventHandler::new(
                    Arc::clone(&self.client),
                    Arc::clone(&client_ready_notify),
                ))
                .await?;

            self.cache = Some(Arc::clone(&client.cache));
            self.data = Some(Arc::clone(&client.data));
            self.http = Some(Arc::clone(&client.http));
            self.shard_manager = Some(Arc::clone(&client.shard_manager));
            if let Some(shard_manager) = &self.shard_manager {
                self.shard_manager = Some(Arc::clone(shard_manager));
            }
            if let Some(voice_manager) = &self.voice_manager {
                self.voice_manager = Some(Arc::clone(voice_manager));
            }
            self.ws_url = Some(Arc::clone(&client.ws_url));

            info!("Connecting to Discord");
            let client_handle = tokio::spawn(async move { client.start().await });

            // This prevents waiting for the timeout if the client fails immediately
            // TODO: Optimize this, as it will currently add 1000mqs to the startup time
            sleep(Duration::from_secs(1)).await;
            if client_handle.is_finished() {
                client_handle.await??;
                return Err("Discord client stopped unexpectedly and with no error".into());
            }

            if timeout(self.connection_timeout, client_ready_notify.notified())
                .await
                .is_err()
            {
                client_handle.abort();
                let result = convert_thread_result(client_handle).await;
                result?;

                return Err(format!(
                    "Discord client failed to connect within {} seconds",
                    self.connection_timeout.as_secs()
                )
                .into());
            }

            self.client_handle = Some(client_handle);
            Ok(())
        })
    }

    fn stop(&mut self) -> PinnedBoxedFutureResult<'_, ()> {
        Box::pin(async move {
            if let Some(client_handle) = self.client_handle.take() {
                info!("Waiting for Discord client to stop...");

                client_handle.abort();
                let result = convert_thread_result(client_handle).await;
                result?;
            }

            info!("Discord client stopped");
            Ok(())
        })
    }
}

// If the thread ended WITHOUT a JoinError from aborting, the client already stopped unexpectedly
async fn convert_thread_result(client_handle: JoinHandle<Result<(), Error>>) -> Result<(), Error> {
    match client_handle.await {
        Ok(result) => result,
        Err(_) => Ok(()),
    }
}

struct EventHandler {
    client: Arc<Mutex<Option<Ready>>>,
    ready_notify: Arc<Notify>,
}

impl EventHandler {
    pub fn new(client: Arc<Mutex<Option<Ready>>>, ready_notify: Arc<Notify>) -> Self {
        Self {
            client,
            ready_notify,
        }
    }
}

#[async_trait]
impl client::EventHandler for EventHandler {
    async fn ready(&self, _ctx: Context, data_about_bot: Ready) {
        info!("Connected to Discord as {}", data_about_bot.user.tag());
        *self.client.lock().await = Some(data_about_bot);
        self.ready_notify.notify_one();
    }
}
