use super::{types::LifetimedPinnedBoxedFutureResult, Priority, Service, ServiceInfo, ServiceManager};
use log::{error, info, warn};
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
use std::{
    sync::{Arc, OnceLock},
    time::Duration,
};
use tokio::{
    select, spawn,
    sync::{Mutex, Notify, RwLock},
    task::JoinHandle,
    time::sleep,
};

//TODO: Restructure
pub struct DiscordService {
    info: ServiceInfo,
    discord_token: String,
    pub ready: Arc<OnceLock<Ready>>,
    client_handle: Option<JoinHandle<Result<(), Error>>>,
    pub cache: OnceLock<Arc<Cache>>,
    pub data: OnceLock<Arc<RwLock<TypeMap>>>,
    pub http: OnceLock<Arc<Http>>,
    pub shard_manager: OnceLock<Arc<ShardManager>>,
    pub voice_manager: OnceLock<Arc<dyn VoiceGatewayManager>>,
    pub ws_url: OnceLock<Arc<Mutex<String>>>,
}

impl DiscordService {
    pub fn new(discord_token: &str) -> Self {
        Self {
            info: ServiceInfo::new("lum_builtin_discord", "Discord", Priority::Essential),
            discord_token: discord_token.to_string(),
            ready: Arc::new(OnceLock::new()),
            client_handle: None,
            cache: OnceLock::new(),
            data: OnceLock::new(),
            http: OnceLock::new(),
            shard_manager: OnceLock::new(),
            voice_manager: OnceLock::new(),
            ws_url: OnceLock::new(),
        }
    }
}

impl Service for DiscordService {
    fn info(&self) -> &ServiceInfo {
        &self.info
    }

    fn start(&mut self, _service_manager: Arc<ServiceManager>) -> LifetimedPinnedBoxedFutureResult<'_, ()> {
        Box::pin(async move {
            let client_ready_notify = Arc::new(Notify::new());

            let framework = StandardFramework::new();
            framework.configure(Configuration::new().prefix("!"));

            let mut client = Client::builder(self.discord_token.as_str(), GatewayIntents::all())
                .framework(framework)
                .event_handler(EventHandler::new(
                    Arc::clone(&self.ready),
                    Arc::clone(&client_ready_notify),
                ))
                .await?;

            if self.cache.set(Arc::clone(&client.cache)).is_err() {
                error!("Could not set cache OnceLock because it was already set. This should never happen.");
                return Err("Could not set cache OnceLock because it was already set.".into());
            }

            if self.data.set(Arc::clone(&client.data)).is_err() {
                error!("Could not set data OnceLock because it was already set. This should never happen.");
                return Err("Could not set data OnceLock because it was already set.".into());
            }

            if self.http.set(Arc::clone(&client.http)).is_err() {
                error!("Could not set http OnceLock because it was already set. This should never happen.");
                return Err("Could not set http OnceLock because it was already set.".into());
            }

            if self.shard_manager.set(Arc::clone(&client.shard_manager)).is_err() {
                error!("Could not set shard_manager OnceLock because it was already set. This should never happen.");
                return Err("Could not set shard_manager OnceLock because it was already set.".into());
            }

            if let Some(voice_manager) = &client.voice_manager {
                if self.voice_manager.set(Arc::clone(voice_manager)).is_err() {
                    error!("Could not set voice_manager OnceLock because it was already set. This should never happen.");
                    return Err("Could not set voice_manager OnceLock because it was already set.".into());
                }
            } else {
                warn!("Voice manager is not available");
            }

            if self.ws_url.set(Arc::clone(&client.ws_url)).is_err() {
                error!("Could not set ws_url OnceLock because it was already set. This should never happen.");
                return Err("Could not set ws_url OnceLock because it was already set.".into());
            }

            let client_handle = spawn(async move { client.start().await });

            select! {
                _ = client_ready_notify.notified() => {},
                _ = sleep(Duration::from_secs(2)) => {},
            }

            if client_handle.is_finished() {
                client_handle.await??;
                return Err("Discord client stopped unexpectedly".into());
            }

            self.client_handle = Some(client_handle);
            Ok(())
        })
    }

    fn stop(&mut self) -> LifetimedPinnedBoxedFutureResult<'_, ()> {
        Box::pin(async move {
            if let Some(client_handle) = self.client_handle.take() {
                info!("Waiting for Discord client to stop...");

                client_handle.abort(); // Should trigger a JoinError in the client_handle, if the task hasn't already ended

                // If the thread ended WITHOUT a JoinError, the client already stopped unexpectedly
                let result = async move {
                    match client_handle.await {
                        Ok(result) => result,
                        Err(_) => Ok(()),
                    }
                }
                .await;
                result?;
            }

            Ok(())
        })
    }
}

struct EventHandler {
    client: Arc<OnceLock<Ready>>,
    ready_notify: Arc<Notify>,
}

impl EventHandler {
    pub fn new(client: Arc<OnceLock<Ready>>, ready_notify: Arc<Notify>) -> Self {
        Self { client, ready_notify }
    }
}

#[async_trait]
impl client::EventHandler for EventHandler {
    async fn ready(&self, _ctx: Context, data_about_bot: Ready) {
        info!("Connected to Discord as {}", data_about_bot.user.tag());
        if self.client.set(data_about_bot).is_err() {
            error!("Could not set client OnceLock because it was already set. This should never happen.");
            panic!("Could not set client OnceLock because it was already set");
        }
        self.ready_notify.notify_one();
    }
}
