#![feature(iterator_try_collect, iter_intersperse)]

use std::{
    fmt::Display,
    path::{Path, PathBuf},
    str::FromStr,
};

use clap::Parser;
use uuid::Uuid;

mod connection;
mod runner;

#[derive(Debug, Parser)]
pub struct Opts {
    #[arg(long, required = true)]
    remote: String,

    #[arg(long, required = true)]
    id: String,

    // Expects ttyXYZ:<bus>-<ports>.*
    #[arg(short = 'd', long = "device", required = true)]
    devices: Vec<String>,

    #[arg(long, required = true)]
    test_runner: PathBuf,
}

#[derive(Debug)]
struct DeviceOpt {
    serial: PathBuf,
    bus: u8,
    ports: Vec<u8>,
}
impl FromStr for DeviceOpt {
    type Err = eyre::Report;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let (serial, usb) = s.split_once(':').unwrap();
        let serial = Path::new("/dev").join(serial);
        eyre::ensure!(serial.exists(), "/dev/{} does not exist", serial.display());
        let (bus, ports) = usb.split_once('-').unwrap();
        let bus = u8::from_str_radix(bus, 10)?;
        let ports = ports
            .split('.')
            .map(|s| u8::from_str_radix(s, 10))
            .try_collect()?;
        Ok(Self { serial, bus, ports })
    }
}
impl Display for DeviceOpt {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}:{}-{}",
            self.serial.display(),
            self.bus,
            self.ports
                .iter()
                .map(|x| format!("{x}"))
                .intersperse(".".into())
                .collect::<String>()
        )
    }
}

#[derive(Debug)]
pub struct DeviceCtl {
    pub serial: PathBuf,
    pub usb_dev: yusb::Device,
}

#[tokio::main(flavor = "multi_thread", worker_threads = 10)]
async fn main() {
    tracing_subscriber::fmt::init();

    let opts = Opts::parse();

    let mut opt_devices: Vec<DeviceOpt> = opts
        .devices
        .iter()
        .map(AsRef::as_ref)
        .map(DeviceOpt::from_str)
        .try_collect()
        .unwrap();

    let mut ctl_devices = vec![];

    let devices = yusb::devices().unwrap();
    for dev in devices.iter() {
        if let Some(i) = opt_devices.iter().position(|dev_opt| {
            dev.port_numbers()
                .map(|port_numbers| port_numbers == dev_opt.ports)
                .unwrap_or(false)
                && dev.bus_number() == dev_opt.bus
        }) {
            let DeviceOpt {
                serial,
                bus: _,
                ports: _,
            } = opt_devices.swap_remove(i);
            println!(
                "{}@{}-{:?}",
                dev.address(),
                dev.bus_number(),
                dev.port_numbers()
            );
            ctl_devices.push(DeviceCtl {
                serial,
                usb_dev: dev,
            });
        }
    }

    if !opt_devices.is_empty() {
        for dev_opt in opt_devices {
            tracing::error!("Unable to discover device {dev_opt}!");
        }
        return;
    }

    // spawn runner workers

    let mut queues = vec![];
    let mut handles = vec![];
    for dev_ctl in ctl_devices {
        let (ctl_tx, ctl_rx) = tokio::sync::mpsc::channel(1);
        let (msg_tx, msg_rx) = tokio::sync::mpsc::channel(1);
        let jh = tokio::spawn(runner::worker(
            Uuid::new_v4(),
            dev_ctl,
            ctl_rx,
            msg_tx,
            opts.test_runner.clone(),
        ));
        queues.push((ctl_tx, msg_rx));
        handles.push(jh);
    }

    // spawn connection worker
    if let Err(e) = connection::connect(opts.remote, opts.id, queues).await {
        tracing::error!("Connection worker failed with error: {e:?}");
    }
    // at this point, the queues have all dropped, so runner workers should also go down
    for hdl in handles {
        if let Err(e) = hdl.await {
            tracing::error!("Error waiting for runner worker task handle: {e:?}");
        }
    }
}
