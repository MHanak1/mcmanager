use std::sync::{Arc};
use log::{debug, error};
use serde::{Deserialize, Serialize};
use socketioxide::extract::{Data, SocketRef, State};
use tokio::sync::Mutex;
use uuid::Uuid;
use crate::api::serve::AppState;
use crate::database::objects::{Session, User};
use crate::database::types::Id;

#[derive(Debug, Deserialize)]
pub struct ConnectValues{
    ticket: Uuid,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ConnectionStatus {
    Connected(Id),
    Disconnected,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ConnectionError {
    NotFound,
    InvalidTicket,
}

pub async fn console_socketio(
    socket: SocketRef,
    Data(data): Data<ConnectValues>,
    State(state): State<AppState>
) {
    socket.emit("hello", "hiiii").expect("TODO: panic message");

    let session_id = if let Some(session_id) = state.console_tickets.get(&data.ticket).await {
        session_id
    } else {
        _ = socket.emit("error", &ConnectionError::InvalidTicket);
        socket.disconnect().expect("could not close socket");
        return;
    };

    let session = if let Ok(session) = state.database.get_one::<Session>(session_id, None).await {
        session
    } else {
        _ = socket.emit("error", &ConnectionError::InvalidTicket);
        socket.disconnect().expect("could not close socket");
        return;
    };

    let user: User = state.database.get_one(session.user_id, None).await.expect("user not found");

    let connected = Arc::new(Mutex::new(None));
    #[derive(Deserialize)]
    struct SubscribeData {
        id: Id,
    }

    socket.on("subscribe", {
        let send_task = connected.clone();
        let state = state.clone();
        let user = user.clone();
        async move |s: SocketRef, Data::<SubscribeData>(data)| {
            let id = data.id;
            debug!("SocketIO subscribe: {id}");
            if let Ok(world) = state.database.get_one(id, Some((&user, &user.group(state.database.clone(), None).await))).await {
                let server = state.servers.get_or_create_server(&world).await.expect("could not get server");
                let mut stdout = server.lock().await.stdout();
                send_task.lock().await.replace((tokio::task::spawn_blocking({
                    let s = s.clone();
                    move || {
                        while let Ok(log) = stdout.blocking_recv() {
                            if s.emit("console", &log).is_err() {
                                return;
                            }
                        }
                    }
                }), id));
                let _ = s.emit("status", &ConnectionStatus::Connected(id));
            } else {
                _ = s.emit("error", &ConnectionError::NotFound);
            }
        }
    });

    socket.on("unsubscribe", {
        let connected = connected.clone();
        async move |s: SocketRef| {
            debug!("SocketIO unsubscribe");
            connected.lock().await.take();
            let _ = s.emit("status", &ConnectionStatus::Disconnected);
        }
    });

    #[derive(Deserialize)]
    struct CommandData {
        command: String,
    }
    socket.on("command", {
        async move |s: SocketRef, Data::<CommandData>(data)| {
            let command = data.command;
            debug!("SocketIO command: {command}");
            if let Some((_, id)) = *connected.lock().await {
                if let Ok(world) = state.database.get_one(id, Some((&user, &user.group(state.database.clone(), None).await))).await {
                    let server = state.servers.get_or_create_server(&world).await.expect("could not get server");
                    if let Err(err) = server.lock().await.write_console(format!("{command}\n")).await {
                        error!("{err}")
                    }
                }
            } else {
                let _ = s.emit("status", &ConnectionStatus::Disconnected);
                error!("could not get server");
            }
        }
    });

}