use std::future::Future;
use zbus::{Connection, zvariant::OwnedObjectPath};

use kw_utils::conv::u64_to_u32_saturating;

use super::{
    proxies::{SystemdManagerProxy, SystemdServiceProxy, SystemdUnitProxy},
    types::{RawUnit, ServiceDetails, ServiceProperties, UnitProperties},
    utils::block_on_local,
};
use crate::prelude::*;

/// What the monitor needs from D-Bus, so its diff logic can run on a fake in tests.
pub trait UnitSource: Send + Sync + 'static {
    fn list_units(&self) -> impl Future<Output = Result<Vec<RawUnit>>> + Send;
    /// Extended properties of an active service. Missing ones are `None`.
    fn service_details(&self, unit_name: &str, object_path: &OwnedObjectPath) -> ServiceDetails;
}

pub struct DbusSource(pub Connection);

impl UnitSource for DbusSource {
    async fn list_units(&self) -> Result<Vec<RawUnit>> {
        let manager = SystemdManagerProxy::new(&self.0)
            .await
            .map_err(|e| Error::Systemd(format!("Failed to create manager proxy: {e}")))?;

        manager
            .list_units()
            .await
            .map_err(|e| Error::Systemd(format!("ListUnits() failed: {e}")))
    }

    /// Delegates to two focused helpers — one per D-Bus interface — so a
    /// missing interface on one doesn't affect the other.
    fn service_details(&self, unit_name: &str, object_path: &OwnedObjectPath) -> ServiceDetails {
        let (fragment_path, since) = self.fetch_unit_properties(unit_name, object_path);
        let (main_pid, memory_bytes, cpu_usage_ns, restart_count) =
            self.fetch_service_properties(object_path);

        (
            main_pid,
            memory_bytes,
            cpu_usage_ns,
            restart_count,
            since,
            fragment_path,
        )
    }
}

impl DbusSource {
    /// Reads `org.freedesktop.systemd1.Unit` properties via the object path
    /// returned by `ListUnits()`. Both calls are independent so a missing
    /// property on one doesn't suppress the other.
    fn fetch_unit_properties(
        &self,
        unit_name: &str,
        object_path: &OwnedObjectPath,
    ) -> UnitProperties {
        // Build proxy once
        let Some(proxy) = SystemdUnitProxy::builder(&self.0)
            .path(object_path.clone())
            .map_err(|e| debug!(?e, unit_name, "could not build unit proxy"))
            .ok()
            .and_then(|b| block_on_local(async { b.build().await.ok() }))
        else {
            return (None, None);
        };
        block_on_local(async {
            let fragment_path = proxy.fragment_path().await.ok();

            let since = proxy
                .active_enter_timestamp()
                .await
                .ok()
                .and_then(since_from_usec);

            (fragment_path, since)
        })
    }

    /// Reads `org.freedesktop.systemd1.Service` properties.
    /// All four properties are fetched in a single proxy session.
    fn fetch_service_properties(&self, object_path: &OwnedObjectPath) -> ServiceProperties {
        let svc_proxy = SystemdServiceProxy::builder(&self.0)
            .path(object_path.clone())
            .ok()
            .and_then(|b| block_on_local(async { b.build().await.ok() }));

        svc_proxy.map_or((None, None, None, None), |svc| {
            block_on_local(async {
                let pid = svc.main_pid().await.ok().filter(|&p| p != 0);
                let mem = svc.memory_current().await.ok().filter(|&m| m != u64::MAX);
                let cpu = svc
                    .c_p_u_usage_n_sec()
                    .await
                    .ok()
                    .filter(|&c| c != u64::MAX);
                let restarts = svc.n_restarts().await.ok();
                (pid, mem, cpu, restarts)
            })
        })
    }
}

/// `ActiveEnterTimestamp` as rfc3339. Zero means the unit never entered active.
pub fn since_from_usec(ts_usec: u64) -> Option<String> {
    if ts_usec == 0 {
        return None;
    }
    let secs = ts_usec / 1_000_000;
    let nsecs = u64_to_u32_saturating((ts_usec % 1_000_000).saturating_mul(1000));
    chrono::DateTime::from_timestamp(secs.cast_signed(), nsecs).map(|dt| dt.to_rfc3339())
}
