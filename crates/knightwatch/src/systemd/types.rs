use zbus::zvariant::OwnedObjectPath;

/// Raw unit tuple returned by ListUnits():
/// (name, description, load_state, active_state, sub_state, following,
///  object_path, job_id, job_type, job_object_path)
pub type RawUnit = (
    String,
    String,
    String,
    String,
    String,
    String,
    OwnedObjectPath,
    u32,
    String,
    OwnedObjectPath,
);
pub type ServiceDetails = (
    Option<u32>,
    Option<u64>,
    Option<u64>,
    Option<u32>,
    Option<String>,
    Option<String>,
);
pub type ServiceProperties = (Option<u32>, Option<u64>, Option<u64>, Option<u32>);
pub type UnitProperties = (Option<String>, Option<String>);