use nova_kernel::{ErrorCategory, NovaError, Result};

pub type PluginResult<T> = Result<T>;

pub fn plugin_error(code: &str, message: &str) -> NovaError {
    NovaError::new(ErrorCategory::Plugin, code, message)
}
