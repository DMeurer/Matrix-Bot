mod access;
mod alerts;
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
use std::{env, path::PathBuf, sync::Arc};
use tokio::sync::Mutex;

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

    // Alert notification template
    let alert_template = Arc::new(
        env::var("ALERT_TEMPLATE").unwrap_or_else(|_| {
            "Alert {name} changed\n\nOld: {old}\nNew: {new}".to_string()
        }),
    );

    // Alert database
    let alert_db = alerts::AlertDb::open(&session_dir.join("alerts.db"))
        .expect("Failed to open alerts database");

    // Build the access-control list from ADMIN_USERS env var + persisted allowed_users.json
    let admins: Vec<String> = env::var("ADMIN_USERS")
        .unwrap_or_default()
        .split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();
    if admins.is_empty() {
        tracing::warn!("ADMIN_USERS is not set — no one can use restricted commands");
    }
    let access = Arc::new(Mutex::new(access::AccessControl::load(
        session_dir.join("allowed_users.json"),
        admins,
    )));

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
        let access = Arc::clone(&access);
        let alert_db = Arc::clone(&alert_db);
        let alert_template = Arc::clone(&alert_template);
        move |ev: OriginalSyncRoomMessageEvent, room: Room, client: Client| {
            let bot_name = bot_name.clone();
            let access = Arc::clone(&access);
            let alert_db = Arc::clone(&alert_db);
            let alert_template = Arc::clone(&alert_template);
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
                // body_original preserves case for argument values (e.g. innerText, href)
                // body_lower is used only for command routing
                let body_original = raw_body.trim_start_matches('\u{202E}').trim();
                let body_lower = body_original.to_lowercase();
                let is_direct = room.is_direct().await.unwrap_or(false);

                // Returns true if `body` is exactly `cmd` or starts with `cmd ` (with a space).
                // Prevents "mensa????" from matching "mensa".
                let is_cmd = |body: &str, cmd: &str| {
                    body == cmd || body.starts_with(&format!("{cmd} "))
                };

                // In group rooms require the bot name prefix and strip it before matching.
                // In DMs no prefix is needed.
                // match_body  — lowercase, used for routing decisions
                // handle_body — original case, passed to command handlers so argument values
                //               like "innerText" or "href" are not silently lowercased
                let prefix = format!("{bot_name} ");
                let (match_body, handle_body): (&str, &str) = if is_direct {
                    (&body_lower, body_original)
                } else {
                    if body_lower.starts_with(&prefix) {
                        (&body_lower[prefix.len()..], &body_original[prefix.len()..])
                    } else {
                        return;
                    }
                };

                // Public commands: no auth required, work in DMs and group rooms.
                let is_public_cmd = is_cmd(match_body, "mensa")
                    || is_cmd(match_body, "help");
                // Restricted commands: require the sender to be on the allowed list.
                let is_restricted_cmd = is_cmd(match_body, "alerts")
                    || is_cmd(match_body, "allow")
                    || is_cmd(match_body, "disallow");

                if !is_public_cmd && !is_restricted_cmd {
                    return;
                }

                let sender = ev.sender.as_str();
                let room_id = room.room_id().as_str();

                tracing::debug!(
                    "Command from {} in {}: {:?}",
                    sender, room_id, handle_body
                );

                let mut response = if match_body.starts_with("mensa") {
                    commands::handle_mensa(handle_body).await
                } else if match_body.starts_with("help") {
                    let show_restricted = access.lock().await.is_allowed(sender);
                    commands::handle_help(handle_body, show_restricted)
                } else {
                    // Restricted — verify sender is allowed
                    let is_allowed = access.lock().await.is_allowed(sender);
                    if !is_allowed {
                        tracing::warn!("Unauthorised command attempt by {sender}: {handle_body:?}");
                        "Du bist nicht berechtigt, diesen Befehl zu verwenden.".to_string()
                    } else if match_body.starts_with("alerts") {
                        alerts::commands::handle_alerts(
                            handle_body, sender, room_id,
                            &alert_db, &client, &alert_template,
                        ).await
                    } else if match_body.starts_with("allow") {
                        let mut ac = access.lock().await;
                        commands::handle_allow(handle_body, &mut ac)
                    } else if match_body.starts_with("disallow") {
                        let mut ac = access.lock().await;
                        commands::handle_disallow(handle_body, &mut ac, &alert_db).await
                    } else {
                        return;
                    }
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

    // Launch the alert scheduler background task
    {
        let db = Arc::clone(&alert_db);
        let tmpl = Arc::clone(&alert_template);
        let c = client.clone();
        tokio::spawn(async move {
            alerts::scheduler::run_scheduler(db, c, tmpl).await;
        });
    }

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
