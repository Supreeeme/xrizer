use libmonado::{Device, DeviceRole, Monado};
use log::{info, warn};

use crate::openxr_data::Hand;

use openvr as vr;

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

    pub fn get_device_from_vr_index(&self, device_index: u32) -> Option<Device<'_>> {
        return match device_index {
            vr::k_unTrackedDeviceIndex_Hmd => self.0.device_from_role(DeviceRole::Head).ok(),
            x if Hand::try_from(x).is_ok() => match Hand::try_from(x).unwrap() {
                Hand::Left => self.0.device_from_role(DeviceRole::Left).ok(),
                Hand::Right => self.0.device_from_role(DeviceRole::Right).ok(),
            },
            _ => None,
        };
    }
}
