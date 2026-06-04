mod commands;
mod mensa;

use anyhow::Result;
use matrix_sdk::{
    Client, Room, RoomState,
    config::SyncSettings,
    matrix_auth::MatrixSession,
    ruma::{
        MilliSecondsSinceUnixEpoch,
        events::{
            room::member::StrippedRoomMemberEvent,
            room::message::{
                MessageType, OriginalSyncRoomMessageEvent, RoomMessageEventContent,
            },
        },
    },
};
use std::{env, path::PathBuf};

#[tokio::main]
async fn main() -> Result<()> {
    dotenvy::dotenv().ok();

    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let homeserver_url = env::var("MATRIX_HOMESERVER")
        .expect("MATRIX_HOMESERVER must be set");
    let username = env::var("MATRIX_USERNAME")
        .expect("MATRIX_USERNAME must be set");
    let password = env::var("MATRIX_PASSWORD")
        .expect("MATRIX_PASSWORD must be set");

    // Derive bot name from username (@botname:server → "botname") unless overridden
    let bot_name = env::var("BOT_NAME").unwrap_or_else(|_| {
        username
            .trim_start_matches('@')
            .split(':')
            .next()
            .unwrap_or("bot")
            .to_string()
    });

    let session_dir = PathBuf::from("session");
    std::fs::create_dir_all(&session_dir)?;

    let client = Client::builder()
        .homeserver_url(&homeserver_url)
        .sqlite_store(&session_dir, None)
        .build()
        .await?;

    let session_file = session_dir.join("session.json");

    if session_file.exists() {
        let data = std::fs::read_to_string(&session_file)?;
        let session: MatrixSession = serde_json::from_str(&data)?;
        client.restore_session(session).await?;
        tracing::info!("Restored session");
    } else {
        client
            .matrix_auth()
            .login_username(&username, &password)
            .initial_device_display_name("Matrix Bot")
            .await?;

        if let Some(session) = client.matrix_auth().session() {
            std::fs::write(&session_file, serde_json::to_string(&session)?)?;
            tracing::info!("Logged in and saved session");
        }
    }

    tracing::info!("Running as {username}");

    let startup_ts = MilliSecondsSinceUnixEpoch::now();

    client.add_event_handler(
        |ev: StrippedRoomMemberEvent, room: Room, client: Client| async move {
            if ev.state_key != client.user_id().unwrap() {
                return;
            }
            tracing::info!("Joining room {}", room.room_id());
            if let Err(e) = room.join().await {
                tracing::error!("Failed to join room {}: {e}", room.room_id());
            }
        },
    );

    client.add_event_handler({
        let bot_name = bot_name.to_lowercase();
        move |ev: OriginalSyncRoomMessageEvent, room: Room, client: Client| {
            let bot_name = bot_name.clone();
            async move {
                if room.state() != RoomState::Joined {
                    return;
                }

                if ev.origin_server_ts < startup_ts {
                    return;
                }

                if client.user_id().map_or(false, |id| id == ev.sender) {
                    return;
                }

                let MessageType::Text(ref text_content) = ev.content.msgtype else {
                    return;
                };

                let raw_body = text_content.body.trim();
                // U+202E Right-to-Left Override: if present, mirror it onto the response
                let is_rlo = raw_body.starts_with('\u{202E}');
                let body_lower = raw_body
                    .trim_start_matches('\u{202E}')
                    .trim()
                    .to_lowercase();
                let is_direct = room.is_direct().await.unwrap_or(false);

                // Returns true if `body` is exactly `cmd` or starts with `cmd ` (with a space).
                // Prevents "mensa????" from matching "mensa".
                let is_cmd = |body: &str, cmd: &str| {
                    body == cmd || body.starts_with(&format!("{cmd} "))
                };

                // In DMs trigger on "mensa …" / "help …"
                // In group rooms trigger on "<botname> mensa …" / "<botname> help …"
                let command_body: &str = if is_direct {
                    if is_cmd(&body_lower, "mensa") || is_cmd(&body_lower, "help") {
                        body_lower.as_str()
                    } else {
                        return;
                    }
                } else {
                    let prefix = format!("{bot_name} ");
                    if is_cmd(&body_lower, &format!("{bot_name} mensa"))
                        || is_cmd(&body_lower, &format!("{bot_name} help"))
                    {
                        body_lower.strip_prefix(&prefix).unwrap_or(&body_lower)
                    } else {
                        return;
                    }
                };

                let mut response = if command_body.starts_with("mensa") {
                    commands::handle_mensa(command_body).await
                } else {
                    commands::handle_help(command_body)
                };

                if is_rlo {
                    response.insert(0, '\u{202E}');
                }

                if let Err(e) = room
                    .send(RoomMessageEventContent::text_plain(response))
                    .await
                {
                    tracing::error!("Failed to send message: {e}");
                }
            }
        }
    });

    tracing::info!("Starting sync loop");

    // Process one sync cycle first so pending invites arrive and the handler fires
    client.sync_once(SyncSettings::default()).await?;

    // Belt-and-suspenders: join any rooms still showing as invited after sync_once
    for room in client.invited_rooms() {
        tracing::info!("Joining pending invite: {}", room.room_id());
        if let Err(e) = room.join().await {
            tracing::error!("Failed to join {}: {e}", room.room_id());
        }
    }

    client.sync(SyncSettings::default()).await?;

    Ok(())
}
