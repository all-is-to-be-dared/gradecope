use std::{sync::Arc, time::Duration};
use crate::{ServerCtx, sql::SqlUser};
use gradecope_proto::ctl::Ctl;
use tarpc::{
    client, context,
    serde_transport::unix,
    server::{BaseChannel, Channel},
    tokio_serde::formats::Json,
};
use futures::StreamExt;
use tokio::net::unix::UCred;
use users::get_user_by_uid;

async fn spawn(fut: impl Future<Output = ()> + Send + 'static) {
    tokio::spawn(fut);
}

/// PER CONNECTION state
#[derive(Clone)]
struct CtlService {
    credentials: UCred,
    server_ctx: Arc<ServerCtx>
}

impl Ctl for CtlService {
    async fn hi(self, _: context::Context) -> String {
	let user = get_user_by_uid(self.credentials.uid()).unwrap();
	let username = user.name().to_str().unwrap();
	return format!("Hello, {}!", username);
    }
}

pub async fn spawn_socket(server_ctx: Arc<ServerCtx>) -> eyre::Result<()> {
    let mut incoming = unix::listen(&server_ctx.opts.ctl_socket_path, Json::default).await?;
    tokio::spawn(async move {
        while let Some(t) = incoming.next().await {
	    let transport = t.unwrap();
	    let cred = transport.get_ref().peer_cred().expect("Failed to retrieve peer credentials");
	    let service = CtlService { credentials: cred, server_ctx: Arc::clone(&server_ctx) };
            let fut = BaseChannel::with_defaults(transport)
                .execute(service.serve())
                .for_each(spawn);
            tokio::spawn(fut);
        }
    });

    Ok(())
}
