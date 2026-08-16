use zbus::{Result, proxy, zvariant::OwnedObjectPath};

#[proxy(
    interface = "org.freedesktop.systemd1.Manager",
    default_service = "org.freedesktop.systemd1",
    default_path = "/org/freedesktop/systemd1"
)]
pub trait SystemdManager {
    fn start_unit(&self, name: &str, mode: &str) -> Result<OwnedObjectPath>;
    fn stop_unit(&self, name: &str, mode: &str) -> Result<OwnedObjectPath>;
    fn restart_unit(&self, name: &str, mode: &str) -> Result<OwnedObjectPath>;
    fn reload_unit(&self, name: &str, mode: &str) -> Result<OwnedObjectPath>;
    fn get_unit(&self, name: &str) -> Result<OwnedObjectPath>;

    #[zbus(signal)]
    fn job_removed(&self, id: u32, job: OwnedObjectPath, unit: String, result: String);
}

#[proxy(
    interface = "org.freedesktop.systemd1.Unit",
    default_service = "org.freedesktop.systemd1"
)]
pub trait SystemdUnit {
    #[zbus(property)]
    fn active_state(&self) -> Result<String>;
    #[zbus(property)]
    fn sub_state(&self) -> Result<String>;
}
