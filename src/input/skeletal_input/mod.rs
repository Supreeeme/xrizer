pub mod ipc;

use log::debug;
use openxr as xr;
use serde::{Deserialize, Serialize};

macro_rules! skeletal_input_actions {
    ($($field:ident: $ty:ty),+$(,)?) => {
        pub struct SkeletalInputActions {
            $(pub $field: xr::Action<$ty>),+
        }
        #[derive(Serialize, Deserialize, Debug)]
        pub struct SkeletalInputActionStates {
            $(pub $field: $ty),+
        }
        pub struct SkeletalInputBindings {
            $(pub $field: Vec<xr::Path>),+
        }
        impl SkeletalInputBindings {
            pub fn binding_iter(self, actions: &SkeletalInputActions) -> impl Iterator<Item = xr::Binding<'_>> {
                std::iter::empty()
                $(
                    .chain(
                        self.$field.into_iter().map(|binding| xr::Binding::new(&actions.$field, binding))
                    )
                )+
            }
        }
    }
}

skeletal_input_actions! {
    thumb_touch: bool,
    index_touch: bool,
    index_curl: f32,
    rest_curl: f32,
}

pub struct SkeletalInputActionData {
    pub set: xr::ActionSet,
    pub actions: SkeletalInputActions,
}

impl SkeletalInputActionData {
    pub fn new<'a>(instance: &'a xr::Instance, left_hand: xr::Path, right_hand: xr::Path) -> Self {
        debug!("creating skeletal input actions");
        let leftright = [left_hand, right_hand];
        let set = instance
            .create_action_set("xrizer-skeletal-input", "XRizer Skeletal Input", 0)
            .unwrap();
        let thumb_touch = set
            .create_action("thumb-touch", "Thumb Touch", &leftright)
            .unwrap();
        let index_touch = set
            .create_action("index-touch", "Index Touch", &leftright)
            .unwrap();
        let index_curl = set
            .create_action("index-curl", "Index Curl", &leftright)
            .unwrap();
        let rest_curl = set
            .create_action("rest-curl", "Rest Curl", &leftright)
            .unwrap();

        Self {
            set,
            actions: SkeletalInputActions {
                thumb_touch,
                index_touch,
                index_curl,
                rest_curl,
            },
        }
    }
}
