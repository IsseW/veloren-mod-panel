use chrono::Utc;
use rocket::futures::FutureExt;
use std::{
    ops::{Deref, DerefMut},
    sync::Arc,
    thread,
    time::Duration,
};
use tokio::{
    runtime::Runtime,
    sync::mpsc::{error::SendError, Sender},
};
use veloren_client::{addr::ConnectionArgs, Client as VelorenClient, Event as VelorenEvent};
use veloren_common::{
    clock::Clock,
    comp,
    util::{GIT_DATE, GIT_HASH},
};

use crate::MessageType;

const TPS: f64 = 10.0;

async fn connect_to_veloren(
    addr: ConnectionArgs,
    veloren_username: &str,
    veloren_password: &str,
    trusted_auth_server: &str,
    runtime: Arc<Runtime>,
) -> veloren_client::Client {
    let mut retry_cnt = 0u32;
    'connect: loop {
        rocket::debug!("Connecting...");
        let mut mismatched_server_info = None;
        let veloren_client = match VelorenClient::new(
            addr.clone(),
            Arc::clone(&runtime),
            &mut mismatched_server_info,
            veloren_username,
            veloren_password,
            |auth_server| auth_server == trusted_auth_server,
        )
        .await
        {
            Ok(client) => client,
            Err(e) => {
                rocket::error!(
                    "Failed to connect to Veloren server: {:?}, retry: {}",
                    e,
                    retry_cnt
                );
                if let Some(server_info) = mismatched_server_info {
                    rocket::error!(
                        "This is likely due to a version mismatch: Client Version {}-{}, Server Version {}-{}",
                        *GIT_HASH, *GIT_DATE,
                        server_info.git_hash, server_info.git_date
                    )
                }
                retry_cnt += 1;
                tokio::time::sleep(Duration::from_millis(500) * retry_cnt).await;
                continue 'connect;
            }
        };

        rocket::debug!("Logged in.");

        return veloren_client;
    }
}

struct Client {
    sx: Sender<crate::VelorenEvent>,
    client: veloren_client::Client,
    runtime: Arc<Runtime>,
}

impl Deref for Client {
    type Target = veloren_client::Client;

    fn deref(&self) -> &Self::Target {
        &self.client
    }
}

impl DerefMut for Client {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.client
    }
}

impl Client {
    fn send(&self, value: crate::VelorenEvent) -> Result<(), SendError<crate::VelorenEvent>> {
        self.runtime.block_on(self.sx.send(value))
    }
}

impl Drop for Client {
    fn drop(&mut self) {
        for (_, info) in self.player_list() {
            let _ = self.send(crate::VelorenEvent {
                player_alias: info.player_alias.clone(),
                player_uuid: info.uuid,
                time: Utc::now(),
                kind: crate::VelorenEventKind::Activity { online: false },
            });
        }
    }
}

pub fn run(
    addr: ConnectionArgs,
    veloren_username: String,
    veloren_password: String,
    trusted_auth_server: String,
    sx: Sender<crate::VelorenEvent>,
    runtime: Arc<Runtime>,
    mut shutdown: rocket::Shutdown,
) {
    tokio::task::spawn_blocking(move || {
        let mut retry_cnt = 0u32;

        let mut client = Client {
            sx,
            client: runtime.block_on(connect_to_veloren(
                addr.clone(),
                &veloren_username,
                &veloren_password,
                &trusted_auth_server,
                Arc::clone(&runtime),
            )),
            runtime,
        };

        let mut sent_players = false;

        let mut clock = Clock::new(Duration::from_secs_f64(1.0 / TPS));

        loop {
            if (&mut shutdown).now_or_never().is_some() {
                break;
            }
            let events = match client.tick(comp::ControllerInputs::default(), clock.dt(), |_| {}) {
                Ok(events) => events,
                Err(e) => {
                    rocket::error!("Failed to tick client: {:?}, retry: {}", e, retry_cnt);
                    retry_cnt += 1;
                    thread::sleep(Duration::from_secs(10) * retry_cnt);
                    client.client = client.runtime.block_on(connect_to_veloren(
                        addr.clone(),
                        &veloren_username,
                        &veloren_password,
                        &trusted_auth_server,
                        Arc::clone(&client.runtime),
                    ));
                    continue;
                }
            };

            if !sent_players && !client.player_list().is_empty() {
                for (_, info) in client.player_list() {
                    let Ok(_) = client.send(crate::VelorenEvent {
                        player_alias: info.player_alias.clone(),
                        player_uuid: info.uuid,
                        time: Utc::now(),
                        kind: crate::VelorenEventKind::Activity { online: true },
                    }) else {
                        panic!();
                    };
                }
                sent_players = true;
            }
            retry_cnt = 0;

            for event in events {
                match event {
                    VelorenEvent::Chat(msg) => {
                        let message = msg.content().as_plain().unwrap_or("");

                        use veloren_common::comp::chat::ChatType;

                        let send_message = |uid, ty: MessageType| {
                            if let Some(info) = client.player_list().get(&uid) {
                                let message = message
                                    .split_once(':')
                                    .map(|(_, message)| message)
                                    .unwrap_or(&message);

                                let Ok(_) = client.send(crate::VelorenEvent {
                                    player_alias: info.player_alias.clone(),
                                    player_uuid: info.uuid,
                                    time: Utc::now(),
                                    kind: crate::VelorenEventKind::Message { message: message.to_string(), ty }
                                }) else {
                                    return;
                                };
                            }
                        };

                        let send_activity = |uid, online| {
                            if let Some(info) = client.player_list().get(&uid) {
                                let Ok(_) = client.send(crate::VelorenEvent {
                                    player_alias: info.player_alias.clone(),
                                    player_uuid: info.uuid,
                                    time: Utc::now(),
                                    kind: crate::VelorenEventKind::Activity { online },
                                }) else {
                                    return;
                                };
                            }
                        };

                        match msg.chat_type {
                            ChatType::Online(uid) => {
                                send_activity(uid, true);
                            }
                            ChatType::Offline(uid) => {
                                send_activity(uid, false);
                            }
                            ChatType::World(uid) => send_message(uid, MessageType::World),
                            ChatType::Tell(uid, _) => send_message(uid, MessageType::Tell),
                            ChatType::Faction(uid, _) => send_message(uid, MessageType::Faction),
                            _ => {}
                        }
                    }
                    VelorenEvent::Disconnect => {}
                    VelorenEvent::DisconnectionNotification(_) => {
                        rocket::debug!("Will be disconnected soon! :/")
                    }
                    VelorenEvent::Notification(notification) => {
                        rocket::debug!("Notification: {:?}", notification);
                    }
                    _ => {}
                }
            }
            client.cleanup();

            clock.tick();
        }
    });
}

pub fn env_key<T>(key: &str) -> T
where
    T: std::str::FromStr,
    <T as std::str::FromStr>::Err: std::fmt::Debug,
{
    std::env::var(key)
        .unwrap_or_else(|_| panic!("No environment variable '{}' found.", key))
        .parse()
        .unwrap_or_else(|_| panic!("'{}' couldn't be parsed.", key))
}
