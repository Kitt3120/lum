use crate::setlock::SetLock;

use super::{types::LifetimedPinnedBoxedFutureResult, Priority, Service, ServiceInfo, ServiceManager};
use log::{error, info};
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
    select, spawn,
    sync::{Mutex, Notify, RwLock},
    task::JoinHandle,
    time::sleep,
};

//TODO: Restructure
pub struct DiscordService {
    info: ServiceInfo,
    discord_token: String,
    pub ready: Arc<Mutex<SetLock<Ready>>>,
    client_handle: Option<JoinHandle<Result<(), Error>>>,
    pub cache: SetLock<Arc<Cache>>,
    pub data: SetLock<Arc<RwLock<TypeMap>>>,
    pub http: SetLock<Arc<Http>>,
    pub shard_manager: SetLock<Arc<ShardManager>>,
    pub voice_manager: SetLock<Arc<dyn VoiceGatewayManager>>,
    pub ws_url: SetLock<Arc<Mutex<String>>>,
}

impl DiscordService {
    pub fn new(discord_token: &str) -> Self {
        Self {
            info: ServiceInfo::new("lum_builtin_discord", "Discord", Priority::Essential),
            discord_token: discord_token.to_string(),
            ready: Arc::new(Mutex::new(SetLock::new())),
            client_handle: None,
            cache: SetLock::new(),
            data: SetLock::new(),
            http: SetLock::new(),
            shard_manager: SetLock::new(),
            voice_manager: SetLock::new(),
            ws_url: SetLock::new(),
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

            if let Err(error) = self.cache.set(Arc::clone(&client.cache)) {
                return Err(format!("Failed to set cache SetLock: {}", error).into());
            }

            if let Err(error) = self.data.set(Arc::clone(&client.data)) {
                return Err(format!("Failed to set data SetLock: {}", error).into());
            }

            if let Err(error) = self.http.set(Arc::clone(&client.http)) {
                return Err(format!("Failed to set http SetLock: {}", error).into());
            }

            if let Err(error) = self.shard_manager.set(Arc::clone(&client.shard_manager)) {
                return Err(format!("Failed to set shard_manager SetLock: {}", error).into());
            }

            if let Some(voice_manager) = &client.voice_manager {
                if let Err(error) = self.voice_manager.set(Arc::clone(voice_manager)) {
                    return Err(format!("Failed to set voice_manager SetLock: {}", error).into());
                }
            }

            if let Err(error) = self.ws_url.set(Arc::clone(&client.ws_url)) {
                return Err(format!("Failed to set ws_url SetLock: {}", error).into());
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
    client: Arc<Mutex<SetLock<Ready>>>,
    ready_notify: Arc<Notify>,
}

impl EventHandler {
    pub fn new(client: Arc<Mutex<SetLock<Ready>>>, ready_notify: Arc<Notify>) -> Self {
        Self { client, ready_notify }
    }
}

#[async_trait]
impl client::EventHandler for EventHandler {
    async fn ready(&self, _ctx: Context, data_about_bot: Ready) {
        info!("Connected to Discord as {}", data_about_bot.user.tag());
        if let Err(error) = self.client.lock().await.set(data_about_bot) {
            error!("Failed to set client SetLock: {}", error);
            panic!("Failed to set client SetLock: {}", error);
        }
        self.ready_notify.notify_one();
    }
}
