#![allow(dead_code)]

use kw_types::systemd::{UnitActiveState, UnitType};

pub struct UnitFilter {
    pub types: Option<Vec<UnitType>>,
    pub active_states: Option<Vec<UnitActiveState>>,
    pub name_prefix: Option<String>,
    pub include_failed: bool,
}

impl Default for UnitFilter {
    fn default() -> Self {
        Self {
            types: Some(vec![UnitType::Service]),
            active_states: None,
            name_prefix: None,
            include_failed: true,
        }
    }
}

impl UnitFilter {
    /// Returns true if this unit should be included in the snapshot.
    pub fn matches(&self, unit_type: &UnitType, active_state: &str, unit_name: &str) -> bool {
        // Always include failed units if the flag is set
        let is_failed = active_state == "failed";
        if is_failed && self.include_failed {
            return true;
        }

        // Type filter
        if let Some(ref allowed_types) = self.types
            && !allowed_types.iter().any(|t| t == unit_type)
        {
            return false;
        }

        // Active state filter
        if let Some(ref allowed_states) = self.active_states
            && !allowed_states.iter().any(|s| s.as_str() == active_state)
        {
            return false;
        }

        // Name prefix filter
        if let Some(ref prefix) = self.name_prefix
            && !unit_name.starts_with(prefix.as_str())
        {
            return false;
        }

        true
    }
}
