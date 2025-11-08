use libmonado::Monado;
use log::{info, warn};

pub struct SafeMonado(pub Monado);

unsafe impl Send for SafeMonado {}
unsafe impl Sync for SafeMonado {}

impl SafeMonado {
    pub fn safe_connect() -> Option<SafeMonado> {
        let monado = match Monado::auto_connect() {
            Ok(m) => m,
            Err(e) => {
                warn!("Failed to connect to Monado: {}", e);
                return None;
            }
        };

        info!("Connected to Monado! Version: {}", monado.get_api_version());
        Some(SafeMonado(monado))
    }
}