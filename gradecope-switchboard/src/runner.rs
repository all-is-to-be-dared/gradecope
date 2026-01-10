use std::{net::SocketAddr, sync::Arc};

use axum::{
    extract::{ConnectInfo, Query, State, WebSocketUpgrade, ws::WebSocket},
    response::IntoResponse,
};
use futures::SinkExt;
use tokio::task::JoinHandle;

use crate::ServerCtx;

pub struct Handle {
    join_handle: JoinHandle<eyre::Result<()>>,
}

#[derive(Debug, serde::Deserialize)]
struct RouteParams {
    name: String,
}

async fn connected_runner(
    peer_addr: SocketAddr,
    state: Arc<ServerCtx>,
    route_params: RouteParams,
    mut ws: WebSocket,
) {
    let RouteParams { name: peer_name } = route_params;
    tracing::info!("Runner {peer_name} connected from {peer_addr}");

    loop {
        //
    }

    let _ = ws.close().await;
}

#[axum::debug_handler]
async fn websocket_route(
    ConnectInfo(peer_addr): ConnectInfo<SocketAddr>,
    State(state): State<Arc<ServerCtx>>,
    Query(route_params): Query<RouteParams>,
    ws: WebSocketUpgrade,
) -> impl IntoResponse {
    let peer_name = route_params.name.clone();
    ws.on_failed_upgrade(move |e| {
        tracing::error!("Failed websocket upgrade on request from {peer_addr}/{peer_name}: {e:?}");
    })
    .on_upgrade(move |ws| connected_runner(peer_addr, state, route_params, ws))
}

/// Spawns an Axum server that serves a single /runner/control route.
///
/// This runs unauthenticated HTTP, and MUST NOT exposed to the internet; a reverse proxy mTLS
/// termination MUST be used for production deployments.
pub async fn spawn_handler(server_ctx: Arc<ServerCtx>) -> eyre::Result<Handle> {
    let listener = tokio::net::TcpListener::bind(&server_ctx.opts.bind_server).await?;
    let router: axum::Router = axum::Router::new()
        .route("/runner/control", axum::routing::get(websocket_route))
        .with_state(server_ctx);
    let join_handle = tokio::spawn(async move {
        axum::serve(
            listener,
            router.into_make_service_with_connect_info::<SocketAddr>(),
        )
        .await?;

        Ok(())
    });
    Ok(Handle { join_handle })
}
