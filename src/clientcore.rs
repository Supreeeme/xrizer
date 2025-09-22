use crate::{
    applications::Applications,
    chaperone::Chaperone,
    compositor::Compositor,
    extended_display::ExtendedDisplay,
    input::Input,
    misc_unknown::UnknownInterfaces,
    openxr_data::{OpenXrData, RealOpenXrData},
    overlay::OverlayMan,
    overlayview::OverlayView,
    rendermodels::RenderModels,
    screenshots::Screenshots,
    settings::Settings,
    system::System,
};

use openvr::{
    self as vr, IVRClientCore003_Interface, IVRInput010_Interface, Inherits, InterfaceImpl,
    VtableWrapper,
};

use log::{debug, error, info, warn};
use serde::Deserialize;
use std::any::{Any, TypeId};
use std::collections::{hash_map::Entry, HashMap};
use std::ffi::{c_char, c_void, CStr, CString};
use std::sync::{Arc, LazyLock, Mutex, OnceLock, RwLock, Weak};

type ErasedInterface = dyn Any + Sync + Send;

#[repr(transparent)]
#[derive(Default)]
struct InterfaceStore(HashMap<TypeId, Arc<ErasedInterface>>);

impl InterfaceStore {
    fn get<T: 'static + InterfaceImpl>(&self) -> Option<Arc<T>> {
        self.0
            .get(&TypeId::of::<T>())
            .map(|i| i.clone().downcast().unwrap())
    }

    fn entry<T: 'static + InterfaceImpl>(&mut self) -> Entry<'_, TypeId, Arc<ErasedInterface>> {
        self.0.entry(TypeId::of::<T>())
    }

    fn clear(&mut self) {
        self.0.clear();
    }
}

pub enum Vtable {
    V2(VtableWrapper<vr::IVRClientCore002, ClientCore>),
    V3(VtableWrapper<vr::IVRClientCore003, ClientCore>),
}

unsafe impl Sync for Vtable {}
unsafe impl Send for Vtable {}

pub struct ClientCore {
    pub base: OnceLock<Vtable>,
    interface_store: Arc<Mutex<InterfaceStore>>,
    openxr: RwLock<Option<Arc<RealOpenXrData>>>,
}

impl ClientCore {
    pub fn new(version: &CStr) -> Option<Arc<Self>> {
        crate::init_logging();

        if ![c"IVRClientCore_003", c"IVRClientCore_002"].contains(&version) {
            error!("Application requested unknown ClientCore version: {version:?}");
            return None;
        }

        info!("Creating ClientCore version {version:?}");
        let ret = Arc::new(Self {
            base: OnceLock::new(),
            interface_store: Default::default(),
            openxr: RwLock::default(),
        });

        #[allow(clippy::redundant_guards)]
        let base = match version {
            x if x == c"IVRClientCore_003" => {
                Vtable::V3(<Self as Inherits<vr::IVRClientCore003>>::new_wrapped(&ret))
            }
            x if x == c"IVRClientCore_002" => {
                Vtable::V2(<Self as Inherits<vr::IVRClientCore002>>::new_wrapped(&ret))
            }
            _ => unreachable!(),
        };

        assert!(ret.base.set(base).is_ok());
        Some(ret)
    }

    fn try_interface<T, InitFn>(&self, version: &CStr, init: InitFn) -> Option<*mut c_void>
    where
        T: InterfaceImpl + 'static,
        InitFn: FnOnce(&Injector) -> T,
    {
        let get_interface = T::get_version(version)?;
        let mut store = self.interface_store.lock().unwrap();
        let item = match store.entry::<T>() {
            Entry::Occupied(entry) => entry.get().clone(),
            Entry::Vacant(entry) => {
                let injector = Injector {
                    store: self.interface_store.clone(),
                };
                let item = Arc::new(init(&injector));
                entry.insert(item).clone()
            }
        }
        .downcast()
        .unwrap();
        Some(get_interface(&item))
    }
}

impl vr::IVRClientCore002On003 for ClientCore {
    fn Init(&self, app_type: vr::EVRApplicationType) -> vr::EVRInitError {
        <Self as vr::IVRClientCore003_Interface>::Init(self, app_type, std::ptr::null())
    }
}

impl IVRClientCore003_Interface for ClientCore {
    fn Init(
        &self,
        application_type: vr::EVRApplicationType,
        startup_info: *const c_char,
    ) -> vr::EVRInitError {
        if !matches!(
            application_type,
            vr::EVRApplicationType::Scene // Standard apps
            | vr::EVRApplicationType::Background // Proton
        ) {
            error!("Unsupported application type: {application_type:?}");
            return vr::EVRInitError::Init_InvalidApplicationType;
        }

        let manifest_path = (!startup_info.is_null())
            .then(|| {
                let info = unsafe { CStr::from_ptr(startup_info) };

                // The startup info is undocumented, but is used in Proton:
                // https://github.com/ValveSoftware/Proton/blob/1a73b04e6cdf29297c6a79a4098ba17e2bf18872/vrclient_x64/json_converter.cpp#L244
                #[derive(Deserialize)]
                struct StartupInfo {
                    action_manifest_path: CString,
                }

                match serde_json::from_slice::<StartupInfo>(info.to_bytes()) {
                    Ok(info) => Some(info.action_manifest_path),
                    Err(e) => {
                        warn!("Failed to parse startup info: {e:?}");
                        None
                    }
                }
            })
            .flatten();

        match OpenXrData::new(&Injector {
            store: self.interface_store.clone(),
        }) {
            Ok(data) => {
                let data = Arc::new(data);
                if let Some(path) = manifest_path {
                    data.input
                        .force(|_| Input::new(data.clone()))
                        .SetActionManifestPath(path.as_ptr());
                }
                *self.openxr.write().unwrap() = Some(data);

                vr::EVRInitError::None
            }
            Err(e) => {
                error!("Creating OpenXR data failed: {e:?}");
                vr::EVRInitError::Init_VRServiceStartupFailed
            }
        }
    }
    fn Cleanup(&self) {
        self.interface_store.lock().unwrap().clear();

        let mut openxr = self.openxr.write().unwrap();
        assert_eq!(Arc::strong_count(openxr.as_ref().unwrap()), 1);
        openxr.take();
    }
    fn GetIDForVRInitError(&self, _: vr::EVRInitError) -> *const c_char {
        std::ptr::null()
    }
    fn GetEnglishStringForHmdError(&self, _: vr::EVRInitError) -> *const c_char {
        std::ptr::null()
    }
    fn BIsHmdPresent(&self) -> bool {
        true
    }
    fn GetGenericInterface(
        &self,
        name_and_version: *const c_char,
        error: *mut vr::EVRInitError,
    ) -> *mut c_void {
        let interface = unsafe { CStr::from_ptr(name_and_version) };
        debug!("requested interface {interface:?}");

        if !error.is_null() {
            unsafe { *error = vr::EVRInitError::None };
        }

        let openxr = self.openxr.read().unwrap();
        let openxr = openxr.as_ref().unwrap();

        self.try_interface(interface, |injector| System::new(openxr.clone(), injector))
            .or_else(|| {
                self.try_interface(interface, |injector| {
                    Compositor::new(openxr.clone(), injector)
                })
            })
            .or_else(|| self.try_interface(interface, |_| Input::new(openxr.clone())))
            .or_else(|| self.try_interface(interface, |_| RenderModels::default()))
            .or_else(|| {
                self.try_interface(interface, |injector| {
                    OverlayMan::new(openxr.clone(), injector)
                })
            })
            .or_else(|| self.try_interface(interface, |_| Chaperone::new(openxr.clone())))
            .or_else(|| self.try_interface(interface, |_| Applications::default()))
            .or_else(|| self.try_interface(interface, |_| OverlayView::default()))
            .or_else(|| self.try_interface(interface, |_| Screenshots::default()))
            .or_else(|| self.try_interface(interface, |_| Settings::default()))
            .or_else(|| self.try_interface(interface, |_| UnknownInterfaces::default()))
            .or_else(|| self.try_interface(interface, |_| ExtendedDisplay::default()))
            .unwrap_or_else(|| {
                warn!("app requested unknown interface {interface:?}");
                std::ptr::null_mut()
            })
    }
    fn IsInterfaceVersionValid(&self, interface_version: *const c_char) -> vr::EVRInitError {
        // Keep this in sync with GetGenericInterface above.
        static KNOWN_INTERFACES: LazyLock<Box<[&CStr]>> = LazyLock::new(|| {
            [
                System::supported_versions(),
                Compositor::supported_versions(),
                Input::<Compositor>::supported_versions(),
                RenderModels::supported_versions(),
                OverlayMan::supported_versions(),
                Chaperone::supported_versions(),
                Applications::supported_versions(),
                OverlayView::supported_versions(),
                Screenshots::supported_versions(),
                UnknownInterfaces::supported_versions(),
                ExtendedDisplay::supported_versions(),
            ]
            .concat()
            .into_boxed_slice()
        });

        let interface = unsafe { CStr::from_ptr(interface_version) };
        debug!("app asking about interface: {interface:?}");
        if KNOWN_INTERFACES.contains(&interface) {
            vr::EVRInitError::None
        } else {
            warn!("app asked about unknown interface {interface:?}");
            vr::EVRInitError::Init_InvalidInterface
        }
    }
}

#[derive(Default)]
pub struct Injector {
    store: Arc<Mutex<InterfaceStore>>,
}

impl Injector {
    pub fn inject<T: InterfaceImpl>(&self) -> Injected<T> {
        Injected::<T> {
            item: OnceLock::new(),
            store: self.store.clone(),
            _marker: Default::default(),
        }
    }
}

pub struct Injected<T> {
    item: OnceLock<Weak<ErasedInterface>>,
    store: Arc<Mutex<InterfaceStore>>,
    _marker: std::marker::PhantomData<T>,
}

impl<T: InterfaceImpl> Injected<T> {
    #[cfg(test)]
    pub fn set(&self, item: Weak<T>) {
        self.item.set(item).unwrap();
    }
    pub fn get(&self) -> Option<Arc<T>> {
        self.item
            .get()
            .or_else(|| {
                let item: Arc<ErasedInterface> = self.store.lock().unwrap().get::<T>()?;
                self.item
                    .set(Arc::downgrade(&item))
                    .unwrap_or_else(|_| unreachable!());
                Some(self.item.get().unwrap())
            })
            .and_then(|i| Some(i.upgrade()?.downcast().unwrap()))
    }

    pub fn force(&self, init: impl FnOnce(&Injector) -> T) -> Arc<T> {
        self.item
            .get_or_init(|| {
                let injector = Injector {
                    store: self.store.clone(),
                };
                // Item may get forced by another thread while we're in the process of forcing it,
                // so try getting it before inserting one.
                Arc::downgrade(
                    self.store
                        .lock()
                        .unwrap()
                        .entry::<T>()
                        .or_insert_with(|| Arc::new(init(&injector))),
                )
            })
            .upgrade()
            .unwrap()
            .downcast()
            .unwrap()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    impl ClientCore {
        fn get_interface<T: InterfaceImpl + 'static>(&self) -> Option<Arc<T>> {
            self.interface_store.lock().unwrap().get::<T>()
        }
    }

    macro_rules! interface {
        ($interface:ident$(($($fields:tt)*))?, $version:literal) => {
            #[derive(Debug)]
            struct $interface$(($($fields)*))?;
            impl InterfaceImpl for $interface {
                fn get_version(
                    version: &CStr,
                ) -> Option<Box<dyn FnOnce(&Arc<Self>) -> *mut c_void>> {
                    if version == $version {
                        Some(Box::new(|this| Arc::as_ptr(this) as _))
                    } else {
                        None
                    }
                }
                fn supported_versions() -> &'static [&'static CStr] {
                    &[]
                }
            }
        };
    }
    interface!(Interface1, c"one");
    interface!(Interface2(Injected<Interface1>), c"two");
    interface!(Interface3(Injected<Interface1>), c"three");
    interface!(Interface4(Injected<Interface1>), c"four");

    impl<T: std::fmt::Debug + InterfaceImpl> std::fmt::Debug for Injected<T> {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.write_fmt(format_args!(
                "Injected<{}>({:?})",
                std::any::type_name::<T>(),
                self.item.get()
            ))
        }
    }

    #[test]
    fn restart() {
        let core = ClientCore::new(c"IVRClientCore_003").unwrap();
        core.clone()
            .Init(vr::EVRApplicationType::Scene, std::ptr::null());
        core.clone().Cleanup();
        core.clone()
            .Init(vr::EVRApplicationType::Scene, std::ptr::null());
    }

    #[test]
    fn inject() {
        let core = ClientCore::new(c"IVRClientCore_003").unwrap();

        core.try_interface(c"two", |injector| Interface2(injector.inject()));
        let interface2_before = core
            .get_interface::<Interface2>()
            .expect("Interface2 missing from store");

        assert!(
            interface2_before.0.get().is_none(),
            "Interface1 should not have been injected yet"
        );

        core.try_interface(c"one", |_| Interface1);
        let interface1 = core
            .get_interface::<Interface1>()
            .expect("Interface1 missing from store");

        let interface2 = core
            .get_interface::<Interface2>()
            .expect("Interface2 missing from store");
        assert_eq!(
            Arc::as_ptr(&interface2),
            Arc::as_ptr(&interface2_before),
            "Interface2 changed in store"
        );

        let injected = interface2.0.get().expect("Interface1 not injected");
        assert_eq!(Arc::as_ptr(&injected), Arc::as_ptr(&interface1),);
    }

    #[test]
    fn inject_force() {
        let core = ClientCore::new(c"IVRClientCore_003").unwrap();

        core.try_interface(c"two", |injector| Interface2(injector.inject()));
        let interface2 = core
            .get_interface::<Interface2>()
            .expect("Interface2 missing from store");
        assert!(
            interface2.0.get().is_none(),
            "Interface1 should not have been injected yet"
        );
        core.try_interface(c"three", |injector| Interface3(injector.inject()));
        let interface3 = core
            .get_interface::<Interface3>()
            .expect("Interface3 missing from store");
        assert!(
            interface3.0.get().is_none(),
            "Interface1 should not have been injected yet"
        );

        interface2.0.force(|_| Interface1);
        let interface1 = core
            .get_interface::<Interface1>()
            .expect("Interface1 not injected");
        let injected2 = interface2
            .0
            .get()
            .expect("Interface1 not injected into Interface2");
        let injected3 = interface3
            .0
            .get()
            .expect("Interface1 not injected into Interface3");
        assert_eq!(Arc::as_ptr(&injected2), Arc::as_ptr(&interface1));
        assert_eq!(Arc::as_ptr(&injected3), Arc::as_ptr(&interface1));

        core.try_interface(c"four", |injector| Interface4(injector.inject()));
        let interface4 = core
            .get_interface::<Interface4>()
            .expect("Interface4 missing from store");
        let injected4 = interface4
            .0
            .get()
            .expect("Interface1 not injected into Interface4");
        assert_eq!(Arc::as_ptr(&injected4), Arc::as_ptr(&interface1));
    }

    #[test]
    fn inject_multithread_force() {
        let core = ClientCore::new(c"IVRClientCore_003").unwrap();

        core.try_interface(c"two", |injector| Interface2(injector.inject()));
        let interface2 = core
            .get_interface::<Interface2>()
            .expect("Interface2 missing from store");

        core.try_interface(c"three", |injector| Interface3(injector.inject()));
        let interface3 = core
            .get_interface::<Interface3>()
            .expect("Interface3 missing from store");

        let (injected2, injected3) = std::thread::scope(|s| {
            let barrier = Arc::new(std::sync::Barrier::new(2));
            let b2 = barrier.clone();
            let i2 = &interface2.0;
            let ij2 = s.spawn(move || {
                b2.wait();
                i2.force(|_| Interface1)
            });
            let i3 = &interface3.0;
            let ij3 = s.spawn(move || {
                barrier.wait();
                i3.force(|_| Interface1)
            });
            (ij2.join().unwrap(), ij3.join().unwrap())
        });

        core.try_interface(c"one", |_| Interface1);
        let interface1 = core
            .get_interface::<Interface1>()
            .expect("Interface1 missing from store");

        assert_eq!(Arc::as_ptr(&injected2), Arc::as_ptr(&interface1));
        assert_eq!(Arc::as_ptr(&injected3), Arc::as_ptr(&interface1));
    }
}
