#![allow(dead_code)]

use serde_json::json;

use kw_types::systemd::{SystemdSnapshot, UnitSnapshot};

#[derive(Debug, Clone)]
pub enum SystemdEvent {
    /// First successful D-Bus enumeration
    InitialSnapshot { snapshot: SystemdSnapshot },
    /// Normal poll tick
    Tick { snapshot: SystemdSnapshot },
    /// A unit transitioned into Failed — wire into Telegram/webhook
    UnitFailed {
        unit_name: String,
        previous_state: kw_types::systemd::UnitActiveState,
    },
    /// A unit recovered out of Failed
    UnitRecovered { unit_name: String },
    /// A unit that wasn't in the list before appeared (e.g. transient unit)
    UnitAppeared { unit: UnitSnapshot },
    /// A unit disappeared entirely (unloaded/transient gone)
    UnitDisappeared { unit_name: String },
}

impl From<&SystemdEvent> for crate::events::EventPayload {
    fn from(event: &SystemdEvent) -> Self {
        let (event_name, data) = match event {
            SystemdEvent::InitialSnapshot { snapshot } => (
                "systemd.initial_snapshot",
                json!({
                    "timestamp": snapshot.timestamp,
                    "unit_count": snapshot.units.len(),
                    "failed_count": snapshot.failed_count,
                    "active_count": snapshot.active_count,
                    "inactive_count": snapshot.inactive_count,
                }),
            ),
            SystemdEvent::Tick { snapshot } => (
                "systemd.tick",
                json!({
                    "timestamp": snapshot.timestamp,
                    "unit_count": snapshot.units.len(),
                    "failed_count": snapshot.failed_count,
                    "active_count": snapshot.active_count,
                    "inactive_count": snapshot.inactive_count,
                }),
            ),
            SystemdEvent::UnitFailed {
                unit_name,
                previous_state,
            } => (
                "systemd.unit_failed",
                json!({
                    "unit_name": unit_name,
                    "previous_state": previous_state.as_str(),
                }),
            ),
            SystemdEvent::UnitRecovered { unit_name } => {
                ("systemd.unit_recovered", json!({ "unit_name": unit_name }))
            }
            SystemdEvent::UnitAppeared { unit } => (
                "systemd.unit_appeared",
                json!({
                    "unit_name": unit.unit_name,
                    "active_state": unit.active_state.as_str(),
                    "sub_state": unit.sub_state,
                    "description": unit.description,
                }),
            ),
            SystemdEvent::UnitDisappeared { unit_name } => (
                "systemd.unit_disappeared",
                json!({ "unit_name": unit_name }),
            ),
        };
        Self::new(crate::events::EventSource::Systemd, event_name, data)
    }
}
