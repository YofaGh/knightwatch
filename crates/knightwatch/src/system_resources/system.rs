#[derive(Debug, Clone, serde::Serialize)]
pub struct StaticHostInfo {
    pub hostname: Option<String>,
    /// OS long name, e.g. "Ubuntu 24.04.1 LTS".
    pub os_name: Option<String>,
    /// Kernel version string.
    pub kernel_version: Option<String>,
    /// CPU architecture, e.g. "`x86_64`", "aarch64".
    pub cpu_arch: Option<String>,
}
