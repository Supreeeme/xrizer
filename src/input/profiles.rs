pub mod knuckles;
pub mod oculus_touch;
pub mod simple_controller;
pub mod vive_controller;
pub mod vive_focus3;
#[cfg(feature = "monado")]
pub mod vive_tracker;
use super::{
    action_manifest::ControllerType, legacy::LegacyBindings, skeletal::SkeletalInputBindings,
};
use crate::input::profiles::typemagic::ContainsPath;
use crate::openxr_data::Hand;
use glam::Mat4;
use knuckles::Knuckles;
use oculus_touch::OculusTouch;
use openxr as xr;
use simple_controller::SimpleController;
use std::ffi::CStr;
use std::marker::PhantomData;
use vive_controller::ViveWands;
use vive_focus3::ViveFocus3;

#[allow(private_interfaces)]
pub trait InteractionProfile: SupportedProfile + Sized + 'static {
    const USE_FORCE_DPAD: bool = false;
    type LegalPaths: LegalPathsT;
    fn profile_path() -> &'static str;
    fn has_required_extensions(enabled_extensions: &xr::ExtensionSet) -> bool;
    fn properties() -> &'static ProfileProperties;
    fn translate_path(_path: DynInputPath) -> Option<DynInputPath> {
        None
    }
    fn legacy_bindings(converter: &InputToXrPath<Self>) -> LegacyBindings;
    fn skeletal_input_bindings(converter: &InputToXrPath<Self>) -> SkeletalInputBindings;
    /// Can be extracted from SteamVR rendermodel files, it is the inverse of the "grip" or "openxr_grip" value
    fn offset_grip_pose(_: Hand) -> Mat4;
}

pub(super) trait RunWithProfile {
    fn run<P: InteractionProfile>(&mut self);
    fn keep_running(&self) -> bool {
        true
    }
}

impl ControllerType {
    pub fn run_for_profile(&self, runner: &mut impl RunWithProfile) {
        // All profiles must be added here to have its actions loaded.
        match self {
            Self::ViveController => {
                runner.run::<ViveWands>();
                runner.run::<SimpleController>();
            }
            Self::OculusTouch => runner.run::<OculusTouch>(),
            Self::Knuckles => runner.run::<Knuckles>(),
            Self::ViveFocus3 => runner.run::<ViveFocus3>(),
            Self::Unknown(_) => {}
        }
    }
}

/// Used to make sure every profile ends up in our profile list.
/// Do not manually implement. Or else...
#[diagnostic::on_unimplemented(
    message = "{Self} must be added to the profile list in `input::profiles::run_for_all_profiles`"
)]
pub trait SupportedProfile {}

// Vive tracker profile is like a fake profile, and when we're doing something in the context
// of all profiles, like suggesting bindings, we typically don't want to do it with the
// tracker profile.
#[cfg(feature = "monado")]
impl SupportedProfile for vive_tracker::ViveTracker {}

pub fn run_for_all_profiles(runner: &mut impl RunWithProfile) {
    macro_rules! profile {
        ($profile:path) => {{
            #[allow(non_local_definitions)]
            impl SupportedProfile for $profile {}
            runner.run::<$profile>();
            if !runner.keep_running() {
                return;
            }
        }};
        () => {};
    }

    profile!(ViveWands);
    profile!(Knuckles);
    profile!(OculusTouch);
    profile!(ViveFocus3);
    profile!(SimpleController);
}

pub struct InputToXrPath<'a, P: InteractionProfile> {
    instance: &'a xr::Instance,
    _marker: std::marker::PhantomData<P>,
}

#[allow(private_bounds)]
impl<'a, P: InteractionProfile> InputToXrPath<'a, P> {
    pub fn new(instance: &'a xr::Instance) -> InputToXrPath<'a, P> {
        Self {
            instance,
            _marker: Default::default(),
        }
    }
    pub fn into<T, M>(&self) -> Vec<xr::Path>
    where
        T: InputPath,
        P::LegalPaths: ContainsPath<T, M>,
    {
        vec![self.instance.string_to_path(&T::DYN.to_string()).unwrap()]
    }

    pub fn leftright<Subpath, Component, M1, M2>(&self) -> Vec<xr::Path>
    where
        Left<Subpath, Component>: InputPath,
        Right<Subpath, Component>: InputPath,
        P::LegalPaths: ContainsPath<Left<Subpath, Component>, M1>,
        P::LegalPaths: ContainsPath<Right<Subpath, Component>, M2>,
    {
        vec![
            self.instance
                .string_to_path(&Left::<Subpath, Component>::DYN.to_string())
                .unwrap(),
            self.instance
                .string_to_path(&Right::<Subpath, Component>::DYN.to_string())
                .unwrap(),
        ]
    }

    pub fn haptics(&self) -> Vec<xr::Path> {
        vec![
            self.instance
                .string_to_path("/user/hand/left/output/haptic")
                .unwrap(),
            self.instance
                .string_to_path("/user/hand/right/output/haptic")
                .unwrap(),
        ]
    }

    pub fn pose(&self) -> Vec<xr::Path> {
        vec![
            self.instance
                .string_to_path("/user/hand/left/input/grip/pose")
                .unwrap(),
            self.instance
                .string_to_path("/user/hand/right/input/grip/pose")
                .unwrap(),
        ]
    }
}

pub trait LegalPathsT {
    fn is_legal(path: DynInputPath) -> bool;
}

impl LegalPathsT for () {
    fn is_legal(_: DynInputPath) -> bool {
        false
    }
}

impl<T: InputPath> LegalPathsT for (T,) {
    #[inline]
    fn is_legal(path: DynInputPath) -> bool {
        path == T::DYN
    }
}
impl<T: InputPath, Tail> LegalPathsT for (T, Tail)
where
    Tail: LegalPathsT,
{
    #[inline]
    fn is_legal(path: DynInputPath) -> bool {
        path == T::DYN || Tail::is_legal(path)
    }
}

macro_rules! legal_paths {
    (
        $(Both::<($both_first_s:ty, $both_first_c:ty) $(, ($both_rest_s:ty, $both_rest_c:ty) )*$(,)?>)?
        $(,Left::<($left_first_s:ty, $left_first_c:ty) $(, ($left_rest_s:ty, $left_rest_c:ty) )*$(,)?>)?
        $(,Right::<($right_first_s:ty, $right_first_c:ty) $(, ($right_rest_s:ty, $right_rest_c:ty) )*$(,)?>)?
    ) => {
        $crate::input::profiles::__recursify_tuple![
            $(
                $crate::input::profiles::Left<$both_first_s, $both_first_c>,
                $crate::input::profiles::Right<$both_first_s, $both_first_c>,
                $(
                    $crate::input::profiles::Left<$both_rest_s, $both_rest_c>,
                    $crate::input::profiles::Right<$both_rest_s, $both_rest_c>,
                )*
            )?
            $(
                $crate::input::profiles::Left<$left_first_s, $left_first_c>,
                $(
                    $crate::input::profiles::Left<$left_rest_s, $left_rest_c>,
                )*
            )?
            $(
                $crate::input::profiles::Right<$right_first_s, $right_first_c>,
                $(
                    $crate::input::profiles::Right<$right_rest_s, $right_rest_c>,
                )*
            )?
        ]
    }
}
use legal_paths;

/// Transforms a tuple (A, B, C) into (A, (B, (C,)))
macro_rules! __recursify_tuple {
    ($current:ty$(,)?) => { ($current,) };
    ($current:ty $(,$rest:ty)*$(,)? ) => {
        ($current, $crate::input::profiles::__recursify_tuple!($($rest),*))
    };
}
use __recursify_tuple;

mod typemagic {
    use super::*;
    // Some type magic for determining if a recursive tuple contains a particular input path.
    // The ideas here are essentially the same as the `HList` type from the `frunk` crate.

    #[diagnostic::on_unimplemented(message = "{T} is not a legal path")]
    pub trait ContainsPath<T, M> {}

    /// A marker for ContainsPath, so we can avoid overlapping blanket implementations.
    trait Marker {}
    /// The type is directly visible
    pub struct Here;
    /// The type is inside of T
    pub struct InsideOf<T>(PhantomData<T>);

    impl Marker for Here {}
    impl<M: Marker> Marker for InsideOf<M> {}

    // Single item tuple: this contains our path
    impl<T: InputPath> ContainsPath<T, Here> for (T,) {}
    impl<T: InputPath, Tail> ContainsPath<T, Here> for (T, Tail) {}
    // Here, `Tail` is a recursive tuple, like (A, (B, (C,)))
    impl<T: InputPath, M: Marker, Head, Tail> ContainsPath<T, InsideOf<M>> for (Head, Tail) where
        Tail: ContainsPath<T, M>
    {
    }
}

pub enum Property<T> {
    BothHands(T),
    PerHand { left: T, right: T },
}

impl<T> Property<T> {
    pub fn get(&self, hand: Hand) -> &T {
        match self {
            Self::BothHands(property) => property,
            Self::PerHand { left, right } => match hand {
                Hand::Left => left,
                Hand::Right => right,
            },
        }
    }
}

pub struct ProfileProperties {
    /// Corresponds to Prop_ModelNumber_String
    /// Can be pulled from a SteamVR System Report
    pub model: Property<&'static CStr>,
    /// Corresponds to Prop_ControllerType_String
    /// Can be pulled from a SteamVR System Report
    pub openvr_controller_type: &'static CStr,
    /// Corresponds to RenderModelName_String
    /// Can be found in SteamVR under resources/rendermodels (some are in driver subdirs)
    pub render_model_name: Property<&'static CStr>,
    pub main_axis: MainAxisType,
    /// Corresponds to Prop_RegisteredDeviceType_String
    pub registered_device_type: Property<&'static CStr>,
    /// Corresponds to Prop_SerialNumber_String
    pub serial_number: Property<&'static CStr>,
    /// Corresponds to Prop_TrackingSystemName_String
    pub tracking_system_name: &'static CStr,
    /// Corresponds to Prop_ManufacturerName_String
    pub manufacturer_name: &'static CStr,
    /// Corresponds to Prop_SupportedButtons_Uint64
    /// Can be pulled from a SteamVR System Report
    pub legacy_buttons_mask: u64,
}

pub enum MainAxisType {
    Thumbstick,
    Trackpad,
}

// Some strong typing for representing input paths.

pub struct Left<Sub = (), Comp = ()>(PhantomData<(Sub, Comp)>);
pub struct Right<Sub = (), Comp = ()>(PhantomData<(Sub, Comp)>);

/// Represents the input subpath of a path, i.e. the `trigger` part of
/// /user/hand/left/input/trigger/click
trait Subpath {
    const DYN: paths::DynSubpath;
}

/// Represents the final component of an input path, i.e. the `click` part
/// of /user/hand/left/input/trigger/click
pub trait Component {
    type Output: xr::ActionInput;
    const DYN: Option<paths::DynComponent>;
}

// Vec2 paths can omit the final component
impl Component for () {
    type Output = xr::Vector2f;
    const DYN: Option<paths::DynComponent> = None;
}

/// Marker trait to signal a legal end component for a given subpath
#[diagnostic::on_unimplemented(message = "{Self} is not a legal component for {P}")]
trait LegalComponentFor<P: Subpath>: Component {}

/// Represents a complete input path: hand, subpath, and component
trait InputPath {
    const DYN: DynInputPath;
}

impl<S, C> InputPath for Left<S, C>
where
    S: Subpath,
    C: LegalComponentFor<S>,
{
    const DYN: DynInputPath = DynInputPath {
        hand: Hand::Left,
        subpath: S::DYN,
        component: C::DYN,
    };
}

impl<S, C> InputPath for Right<S, C>
where
    S: Subpath,
    C: LegalComponentFor<S>,
{
    const DYN: DynInputPath = DynInputPath {
        hand: Hand::Right,
        subpath: S::DYN,
        component: C::DYN,
    };
}

/// A dynamic representation of an input path, typically parsed from a string.
/// i.e.: /user/hand/left/input/trigger/click
/// This is not guaranteed to be an actually valid path!
#[derive(Copy, Clone, PartialEq, Eq)]
pub struct DynInputPath {
    pub hand: Hand,
    pub subpath: paths::DynSubpath,
    pub component: Option<paths::DynComponent>,
}

impl DynInputPath {
    pub fn with_component(self, component: paths::DynComponent) -> Self {
        Self {
            component: Some(component),
            ..self
        }
    }
}

impl std::fmt::Display for DynInputPath {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let hand = match self.hand {
            Hand::Left => "/user/hand/left",
            Hand::Right => "/user/hand/right",
        };
        write!(f, "{hand}/input/{}", self.subpath)?;
        if let Some(component) = &self.component {
            write!(f, "/{component}")?;
        }
        Ok(())
    }
}
impl std::str::FromStr for DynInputPath {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let mut split = s.split('/');
        macro_rules! verify_next {
            ($string:literal) => {
                match split
                    .next()
                    .ok_or_else(|| format!(concat!("missing component ", $string)))?
                {
                    $string => {}
                    other => return Err(format!("expected component {}, got {other}", $string)),
                }
            };
        }

        verify_next!("");
        verify_next!("user");
        verify_next!("hand");

        let hand = match split
            .next()
            .ok_or_else(|| "missing hand component".to_string())?
        {
            "left" => Hand::Left,
            "right" => Hand::Right,
            other => return Err(format!("expected left or right hand, got {other}")),
        };

        verify_next!("input");

        let subpath_str = split.next();
        let subpath = subpath_str
            .and_then(paths::DynSubpath::from_openvr_str)
            .ok_or_else(|| format!("invalid subpath {subpath_str:?}"))?;

        let component = split.next();
        let component = component
            .map(|c| {
                paths::DynComponent::from_openvr_str(c).ok_or_else(|| {
                    format!("invalid component {component:?} for subpath {subpath_str:?}")
                })
            })
            .transpose()?;

        Ok(DynInputPath {
            hand,
            subpath,
            component,
        })
    }
}

pub mod paths {
    use std::fmt::Display;

    use super::*;
    macro_rules! components {
        ($enum:ident, $($name:ident::<$output_type:ty>),+) => {
            $(
                pub struct $name;
                impl Component for $name {
                    type Output = $output_type;
                    const DYN: Option<$enum> = Some($enum::$name);
                }
            )+

            /// Represents the final component of an input path.
            /// The "click" in "/user/hand/input/a/click"
            #[derive(Copy, Clone, PartialEq, Eq)]
            pub enum $enum {
                $($name,)+
            }
        };
    }

    components!(
        DynComponent,
        Click::<bool>,
        Touch::<bool>,
        Value::<f32>,
        Force::<f32>,
        Vec2X::<f32>,
        Vec2Y::<f32>
    );

    impl DynComponent {
        pub fn from_openvr_str(s: &str) -> Option<Self> {
            match s {
                "click" => Some(Self::Click),
                "touch" => Some(Self::Touch),
                "value" | "pull" => Some(Self::Value),
                _ => None,
            }
        }
    }

    impl Display for DynComponent {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            let s = match self {
                Self::Click => "click",
                Self::Touch => "touch",
                Self::Value => "value",
                Self::Force => "force",
                Self::Vec2X => "x",
                Self::Vec2Y => "y",
            };

            f.write_str(s)
        }
    }

    macro_rules! subpaths {
        ($enum:ident, $($name:ident::<$component1:ident $(, $component_rest:ident)*>),+) => {
            $(
                pub struct $name;
                impl Subpath for $name {
                    const DYN: $enum = $enum::$name;
                }
                impl LegalComponentFor<$name> for $component1 {}
                $(
                    impl LegalComponentFor<$name> for $component_rest {}
                )*
            )+

            /// Represents a specific input on an input path
            /// The "a" in "/user/hand/input/a/click"
            #[derive(Copy, Clone, PartialEq, Eq)]
            pub enum $enum {
                $($name,)+
            }

            impl Display for $enum {
                fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                    paste::paste! {
                        let s = match self {
                            $(Self::$name => stringify!([<$name:lower>]),)+
                        };
                    }
                    f.write_str(s)
                }
            }
        }
    }

    subpaths!(
        DynSubpath,
        A::<Click, Touch>,
        B::<Click, Touch>,
        X::<Click, Touch>,
        Y::<Click, Touch>,
        Menu::<Click>,
        Select::<Click>,
        Trigger::<Click, Value, Touch>,
        Squeeze::<Click, Value, Force, Touch>,
        Thumbstick::<Click, Touch, Vec2X, Vec2Y>,
        Trackpad::<Click, Touch, Force, Vec2X, Vec2Y>,
        Thumbrest::<Touch>
    );

    // Vec2 impls
    impl LegalComponentFor<Thumbstick> for () {}
    impl LegalComponentFor<Trackpad> for () {}

    impl DynSubpath {
        pub fn from_openvr_str(s: &str) -> Option<Self> {
            match s {
                "a" => Some(Self::A),
                "b" => Some(Self::B),
                "x" => Some(Self::X),
                "y" => Some(Self::Y),
                "application_menu" | "system" => Some(Self::Menu),
                "trigger" => Some(Self::Trigger),
                "grip" => Some(Self::Squeeze),
                "thumbstick" | "joystick" => Some(Self::Thumbstick),
                "thumbrest" => Some(Self::Thumbrest),
                "trackpad" => Some(Self::Trackpad),
                _ => None,
            }
        }
    }
}
