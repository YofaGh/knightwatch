use zbus::{proxy, zvariant::OwnedObjectPath};

#[proxy(
    interface = "org.freedesktop.systemd1.Manager",
    default_service = "org.freedesktop.systemd1",
    default_path = "/org/freedesktop/systemd1"
)]
pub trait SystemdManager {
    fn list_units(&self) -> zbus::Result<Vec<super::types::RawUnit>>;
    fn get_unit(&self, name: &str) -> zbus::Result<OwnedObjectPath>;
    fn start_unit(&self, name: &str, mode: &str) -> zbus::Result<OwnedObjectPath>;
    fn stop_unit(&self, name: &str, mode: &str) -> zbus::Result<OwnedObjectPath>;
    fn restart_unit(&self, name: &str, mode: &str) -> zbus::Result<OwnedObjectPath>;
    fn reload_unit(&self, name: &str, mode: &str) -> zbus::Result<OwnedObjectPath>;
}

#[proxy(
    interface = "org.freedesktop.systemd1.Unit",
    default_service = "org.freedesktop.systemd1"
)]
pub trait SystemdUnit {
    #[zbus(property)]
    fn active_enter_timestamp(&self) -> zbus::Result<u64>;
    #[zbus(property)]
    fn fragment_path(&self) -> zbus::Result<String>;
}

#[proxy(
    interface = "org.freedesktop.systemd1.Service",
    default_service = "org.freedesktop.systemd1"
)]
pub trait SystemdService {
    #[zbus(property)]
    fn main_pid(&self) -> zbus::Result<u32>;
    #[zbus(property)]
    fn memory_current(&self) -> zbus::Result<u64>;
    #[zbus(property)]
    fn c_p_u_usage_n_sec(&self) -> zbus::Result<u64>;
    #[zbus(property)]
    fn n_restarts(&self) -> zbus::Result<u32>;
}
