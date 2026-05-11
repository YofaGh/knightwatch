#![allow(dead_code)]

use tokio::sync::{broadcast, mpsc, oneshot};

use super::{enums::SystemMonitorQuery, structs::*};

pub fn subscribe_events() -> Option<broadcast::Receiver<super::enums::SystemMonitorEvent>> {
    super::monitor::SYSTEM_MONITOR_EVENT_SENDER
        .get()
        .map(|tx| tx.subscribe())
}

fn get_system_monitor_query_sender() -> Option<&'static mpsc::Sender<SystemMonitorQuery>> {
    super::monitor::SYSTEM_MONITOR_QUERY_SENDER.get()
}

/// Get the current system snapshot.
pub async fn get_snapshot() -> Option<SystemSnapshot> {
    let (tx, rx) = oneshot::channel();
    let _ = get_system_monitor_query_sender()?
        .send(SystemMonitorQuery::Snapshot { response: tx })
        .await;
    rx.await.unwrap_or(None)
}

pub async fn get_cpu() -> Option<CpuSnapshot> {
    let (tx, rx) = oneshot::channel();
    let _ = get_system_monitor_query_sender()?
        .send(SystemMonitorQuery::Cpu { response: tx })
        .await;
    rx.await.unwrap_or(None)
}

pub async fn get_memory() -> Option<MemorySnapshot> {
    let (tx, rx) = oneshot::channel();
    let _ = get_system_monitor_query_sender()?
        .send(SystemMonitorQuery::Memory { response: tx })
        .await;
    rx.await.unwrap_or(None)
}

pub async fn get_disks() -> Vec<DiskSnapshot> {
    let Some(tx_ref) = get_system_monitor_query_sender() else {
        return Vec::new();
    };
    let (tx, rx) = oneshot::channel();
    let _ = tx_ref
        .send(SystemMonitorQuery::Disks { response: tx })
        .await;
    rx.await.unwrap_or_default()
}

pub async fn get_networks() -> Vec<NetworkSnapshot> {
    let Some(tx_ref) = get_system_monitor_query_sender() else {
        return Vec::new();
    };
    let (tx, rx) = oneshot::channel();
    let _ = tx_ref
        .send(SystemMonitorQuery::Networks { response: tx })
        .await;
    rx.await.unwrap_or_default()
}

pub async fn get_gpus() -> Vec<GpuSnapshot> {
    let Some(tx_ref) = get_system_monitor_query_sender() else {
        return Vec::new();
    };
    let (tx, rx) = oneshot::channel();
    let _ = tx_ref.send(SystemMonitorQuery::Gpus { response: tx }).await;
    rx.await.unwrap_or_default()
}

pub async fn get_battery() -> Option<BatterySnapshot> {
    let (tx, rx) = oneshot::channel();
    let _ = get_system_monitor_query_sender()?
        .send(SystemMonitorQuery::Battery { response: tx })
        .await;
    rx.await.unwrap_or(None)
}

pub async fn get_host_info() -> Option<HostInfo> {
    let (tx, rx) = oneshot::channel();
    let _ = get_system_monitor_query_sender()?
        .send(SystemMonitorQuery::HostInfo { response: tx })
        .await;
    rx.await.unwrap_or(None)
}

pub async fn get_temperatures() -> Vec<ThermalSnapshot> {
    let Some(tx_ref) = get_system_monitor_query_sender() else {
        return Vec::new();
    };
    let (tx, rx) = oneshot::channel();
    let _ = tx_ref
        .send(SystemMonitorQuery::Temperatures { response: tx })
        .await;
    rx.await.unwrap_or_default()
}
