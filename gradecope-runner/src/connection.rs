use tokio::sync::mpsc::{Receiver, Sender};

use crate::runner::{WorkerCtl, WorkerMsg};

pub async fn connect(
    remote: String,
    id: String,
    queues: Vec<(Sender<WorkerCtl>, Receiver<WorkerMsg>)>,
) -> eyre::Result<()> {
    todo!()
}
