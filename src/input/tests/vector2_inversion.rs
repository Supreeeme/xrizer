use super::*;
use crate::input::custom_bindings::Vector2Inversion;
use fakexr::ActionState;

const SET: &CStr = c"/actions/inversion";

fn fixture() -> Fixture {
    let mut f = Fixture::new();
    f.load_actions(c"actions_vector2_inversion.json");
    f.set_interaction_profile::<Knuckles>(LeftHand);
    f.set_interaction_profile::<Knuckles>(RightHand);
    f
}

fn sync(f: &mut Fixture) {
    f.sync(vr::VRActiveActionSet_t {
        ulActionSet: f.get_action_set_handle(SET),
        ..Default::default()
    });
}

fn source<P: InteractionProfile>(
    f: &Fixture,
    handle: vr::VRActionHandle_t,
    hand: Hand,
    invert: Vector2Inversion,
    path: &str,
) -> xr::sys::Action {
    let session = f.input.openxr.session_data.get();
    let loaded = session.input_data.get_loaded_actions().unwrap();
    let ActionData::Vector2 { data, .. } = loaded.try_get_action(handle).unwrap() else {
        panic!("expected vector2")
    };
    let profile = f
        .input
        .openxr
        .instance
        .string_to_path(P::profile_path())
        .unwrap();
    let bindings: Vec<_> = data
        .bindings
        .iter()
        .filter(|b| {
            b.profile == profile && b.hand == f.input.get_subaction_path(hand) && b.invert == invert
        })
        .collect();
    assert_eq!(bindings.len(), 1);
    let action = bindings[0].action.as_raw();
    assert_eq!(fakexr::get_suggested_bindings(action, profile), [path]);
    action
}

fn read(f: &Fixture, handle: vr::VRActionHandle_t, hand: u64) -> vr::InputAnalogActionData_t {
    let mut state = Default::default();
    assert_eq!(
        f.input.GetAnalogActionData(
            handle,
            &mut state,
            std::mem::size_of_val(&state) as u32,
            hand
        ),
        vr::EVRInputError::None
    );
    state
}

#[track_caller]
fn check(state: vr::InputAnalogActionData_t, value: [f32; 2], delta: [f32; 2], origin: u64) {
    assert!(state.bActive);
    assert_eq!(state.activeOrigin, origin);
    for (actual, expected) in [state.x, state.y, state.deltaX, state.deltaY]
        .into_iter()
        .zip([value[0], value[1], delta[0], delta[1]])
    {
        assert!(
            (actual - expected).abs() < 0.00001,
            "{actual} != {expected}"
        );
    }
    assert_eq!((state.z, state.deltaZ, state.fUpdateTime), (0.0, 0.0, 0.0));
}

fn axis_case(name: &CStr, invert: Vector2Inversion, sign: [f32; 2]) {
    let mut f = fixture();
    let handle = f.get_action_handle(name);
    let left = f.get_input_source_handle(c"/user/hand/left");
    let action = source::<Knuckles>(
        &f,
        handle,
        Hand::Left,
        invert,
        "/user/hand/left/input/thumbstick",
    );
    assert!(fakexr::check_no_suggested_bindings(
        f.get_action::<xr::Vector2f>(handle),
        f.input
            .openxr
            .instance
            .string_to_path(Knuckles::profile_path())
            .unwrap(),
    ));
    assert!(!read(&f, handle, left).bActive);
    fakexr::set_action_state(action, ActionState::Vector2(0.25, 0.5), LeftHand);
    sync(&mut f);
    for _ in 0..2 {
        check(
            read(&f, handle, left),
            [0.25 * sign[0], 0.5 * sign[1]],
            [0.25 * sign[0], 0.5 * sign[1]],
            left,
        );
    }
    // Pending runtime input must not affect the current OpenVR snapshot.
    fakexr::set_action_state(action, ActionState::Vector2(0.5, 0.25), LeftHand);
    check(
        read(&f, handle, left),
        [0.25 * sign[0], 0.5 * sign[1]],
        [0.25 * sign[0], 0.5 * sign[1]],
        left,
    );
    sync(&mut f);
    check(
        read(&f, handle, left),
        [0.5 * sign[0], 0.25 * sign[1]],
        [0.25 * sign[0], -0.25 * sign[1]],
        left,
    );
    sync(&mut f);
    check(
        read(&f, handle, left),
        [0.5 * sign[0], 0.25 * sign[1]],
        [0.0, 0.0],
        left,
    );
    // History advances even when the application does not read a frame.
    fakexr::set_action_state(action, ActionState::Vector2(0.0, 0.0), LeftHand);
    sync(&mut f);
    fakexr::set_action_state(action, ActionState::Vector2(-0.25, -0.5), LeftHand);
    sync(&mut f);
    check(
        read(&f, handle, left),
        [-0.25 * sign[0], -0.5 * sign[1]],
        [-0.25 * sign[0], -0.5 * sign[1]],
        left,
    );
}

#[test]
fn vector2_inversion_x() {
    axis_case(c"/actions/inversion/in/x", Vector2Inversion::X, [-1.0, 1.0]);
}

#[test]
fn vector2_inversion_y() {
    axis_case(c"/actions/inversion/in/y", Vector2Inversion::Y, [1.0, -1.0]);
}

#[test]
fn vector2_inversion_xy() {
    axis_case(
        c"/actions/inversion/in/xy",
        Vector2Inversion::XY,
        [-1.0, -1.0],
    );
}

#[test]
fn vector2_inversion_absent_empty_unknown_use_direct_bindings() {
    let mut f = fixture();
    let left = f.get_input_source_handle(c"/user/hand/left");
    for name in [
        c"/actions/inversion/in/plain",
        c"/actions/inversion/in/empty",
        c"/actions/inversion/in/unknown",
    ] {
        let handle = f.get_action_handle(name);
        f.verify_bindings::<xr::Vector2f>(
            Knuckles::profile_path(),
            name,
            ["/user/hand/left/input/thumbstick".into()],
        );
        {
            let session = f.input.openxr.session_data.get();
            let ActionData::Vector2 { data, .. } = session
                .input_data
                .get_loaded_actions()
                .unwrap()
                .try_get_action(handle)
                .unwrap()
            else {
                panic!()
            };
            assert!(data.bindings.is_empty());
        }
        fakexr::set_action_state(
            f.get_action::<xr::Vector2f>(handle),
            ActionState::Vector2(0.25, -0.5),
            LeftHand,
        );
        sync(&mut f);
        check(read(&f, handle, left), [0.25, -0.5], [0.25, -0.5], left);
        check(read(&f, handle, 0), [0.25, -0.5], [0.25, -0.5], left);
    }
}

#[test]
fn vector2_inversion_subactions_and_unrestricted() {
    let mut f = fixture();
    let handle = f.get_action_handle(c"/actions/inversion/in/hands");
    let left = f.get_input_source_handle(c"/user/hand/left");
    let right = f.get_input_source_handle(c"/user/hand/right");
    let la = source::<Knuckles>(
        &f,
        handle,
        Hand::Left,
        Vector2Inversion::Y,
        "/user/hand/left/input/thumbstick",
    );
    let ra = source::<Knuckles>(
        &f,
        handle,
        Hand::Right,
        Vector2Inversion::X,
        "/user/hand/right/input/thumbstick",
    );
    assert_ne!(la, ra);
    fakexr::set_action_state(la, ActionState::Vector2(0.25, 0.5), LeftHand);
    // FakeXR allows state on an unbound hand: it must never leak into the output.
    fakexr::set_action_state(la, ActionState::Vector2(1.0, 1.0), RightHand);
    fakexr::set_action_state(ra, ActionState::Vector2(0.75, 0.0), RightHand);
    sync(&mut f);
    for _ in 0..2 {
        check(read(&f, handle, 0), [-0.75, 0.0], [-0.75, 0.0], right);
        check(read(&f, handle, left), [0.25, -0.5], [0.25, -0.5], left);
        check(read(&f, handle, right), [-0.75, 0.0], [-0.75, 0.0], right);
    }
    fakexr::set_action_state(la, ActionState::Vector2(0.5, 0.75), LeftHand);
    sync(&mut f);
    check(read(&f, handle, right), [-0.75, 0.0], [0.0, 0.0], right);
    check(read(&f, handle, left), [0.5, -0.75], [0.25, -0.25], left);
    check(read(&f, handle, 0), [0.5, -0.75], [1.25, -0.75], left);
}

#[test]
fn vector2_inversion_trackpad() {
    let mut f = fixture();
    let handle = f.get_action_handle(c"/actions/inversion/in/trackpad");
    let left = f.get_input_source_handle(c"/user/hand/left");
    let right = f.get_input_source_handle(c"/user/hand/right");
    let action = source::<Knuckles>(
        &f,
        handle,
        Hand::Left,
        Vector2Inversion::Y,
        "/user/hand/left/input/trackpad",
    );
    fakexr::set_action_state(action, ActionState::Vector2(0.25, 0.5), LeftHand);
    sync(&mut f);
    check(read(&f, handle, 0), [0.25, -0.5], [0.25, -0.5], left);
    let state = read(&f, handle, right);
    assert!(!state.bActive);
    assert_eq!(
        (
            state.x,
            state.y,
            state.deltaX,
            state.deltaY,
            state.activeOrigin
        ),
        (0.0, 0.0, 0.0, 0.0, 0)
    );
}

#[test]
fn vector2_inversion_mixed_direct_and_transformed() {
    let mut f = fixture();
    let name = c"/actions/inversion/in/mixed";
    let handle = f.get_action_handle(name);
    let left = f.get_input_source_handle(c"/user/hand/left");
    let right = f.get_input_source_handle(c"/user/hand/right");
    let transformed = source::<Knuckles>(
        &f,
        handle,
        Hand::Left,
        Vector2Inversion::Y,
        "/user/hand/left/input/thumbstick",
    );
    let direct = f.get_action::<xr::Vector2f>(handle);
    assert_ne!(direct, transformed);
    f.verify_bindings::<xr::Vector2f>(
        Knuckles::profile_path(),
        name,
        [
            "/user/hand/left/input/trackpad".into(),
            "/user/hand/right/input/thumbstick".into(),
        ],
    );
    fakexr::set_action_state(direct, ActionState::Vector2(0.0, 0.75), LeftHand);
    fakexr::set_action_state(direct, ActionState::Vector2(0.0, 0.25), RightHand);
    fakexr::set_action_state(transformed, ActionState::Vector2(0.0, 0.5), LeftHand);
    sync(&mut f);
    check(read(&f, handle, left), [0.0, 0.75], [0.0, 0.75], left);
    fakexr::set_action_state(transformed, ActionState::Vector2(0.0, 1.0), LeftHand);
    sync(&mut f);
    check(read(&f, handle, 0), [0.0, -1.0], [0.0, -1.75], left);
    check(read(&f, handle, left), [0.0, -1.0], [0.0, -1.75], left);
    check(read(&f, handle, right), [0.0, 0.25], [0.0, 0.0], right);
    fakexr::deactivate_action(transformed);
    sync(&mut f);
    check(read(&f, handle, left), [0.0, 0.75], [0.0, 1.75], left);
}

#[test]
fn vector2_inversion_multiple_sources_and_same_path() {
    for (name, second_path) in [
        (
            c"/actions/inversion/in/multiple",
            "/user/hand/left/input/trackpad",
        ),
        (
            c"/actions/inversion/in/same_path",
            "/user/hand/left/input/thumbstick",
        ),
    ] {
        let mut f = fixture();
        let handle = f.get_action_handle(name);
        let left = f.get_input_source_handle(c"/user/hand/left");
        let x = source::<Knuckles>(
            &f,
            handle,
            Hand::Left,
            Vector2Inversion::X,
            "/user/hand/left/input/thumbstick",
        );
        let y = source::<Knuckles>(&f, handle, Hand::Left, Vector2Inversion::Y, second_path);
        assert_ne!(x, y);
        fakexr::set_action_state(x, ActionState::Vector2(0.25, 0.0), LeftHand);
        fakexr::set_action_state(y, ActionState::Vector2(0.0, 0.5), LeftHand);
        sync(&mut f);
        check(read(&f, handle, left), [0.0, -0.5], [0.0, -0.5], left);
        fakexr::set_action_state(x, ActionState::Vector2(0.75, 0.0), LeftHand);
        sync(&mut f);
        check(read(&f, handle, left), [-0.75, 0.0], [-0.75, 0.5], left);
    }
}

#[test]
fn vector2_inversion_profile_isolation() {
    let mut f = fixture();
    let handle = f.get_action_handle(c"/actions/inversion/in/profile");
    let left = f.get_input_source_handle(c"/user/hand/left");
    let knuckles = source::<Knuckles>(
        &f,
        handle,
        Hand::Left,
        Vector2Inversion::Y,
        "/user/hand/left/input/thumbstick",
    );
    let wands = source::<ViveWands>(
        &f,
        handle,
        Hand::Left,
        Vector2Inversion::X,
        "/user/hand/left/input/trackpad",
    );
    assert_ne!(knuckles, wands);
    fakexr::set_action_state(knuckles, ActionState::Vector2(0.25, 0.5), LeftHand);
    fakexr::set_action_state(wands, ActionState::Vector2(0.75, 0.0), LeftHand);
    sync(&mut f);
    check(read(&f, handle, left), [0.25, -0.5], [0.25, -0.5], left);
    f.set_interaction_profile::<ViveWands>(LeftHand);
    sync(&mut f);
    check(read(&f, handle, left), [-0.75, 0.0], [-1.0, 0.5], left);
    f.set_interaction_profile::<Knuckles>(LeftHand);
    sync(&mut f);
    check(read(&f, handle, left), [0.25, -0.5], [1.0, -0.5], left);
}

#[test]
fn vector2_inversion_inactive_action_set() {
    let mut f = fixture();
    let handle = f.get_action_handle(c"/actions/inversion/in/y");
    let left = f.get_input_source_handle(c"/user/hand/left");
    let action = source::<Knuckles>(
        &f,
        handle,
        Hand::Left,
        Vector2Inversion::Y,
        "/user/hand/left/input/thumbstick",
    );
    fakexr::set_action_state(action, ActionState::Vector2(0.25, 0.5), LeftHand);
    sync(&mut f);
    f.sync(vr::VRActiveActionSet_t {
        ulActionSet: f.get_action_set_handle(c"/actions/other"),
        ..Default::default()
    });
    for hand in [left, 0] {
        let state = read(&f, handle, hand);
        assert!(!state.bActive);
        assert_eq!((state.x, state.y, state.activeOrigin), (0.0, 0.0, 0));
        assert_eq!((state.deltaX, state.deltaY), (-0.25, 0.5));
    }
    sync(&mut f);
    check(read(&f, handle, left), [0.25, -0.5], [0.25, -0.5], left);
}

#[test]
fn vector2_inversion_preserves_click_touch_dpad() {
    let mut f = fixture();
    let position = f.get_action_handle(c"/actions/inversion/in/y");
    let left = f.get_input_source_handle(c"/user/hand/left");
    let action = source::<Knuckles>(
        &f,
        position,
        Hand::Left,
        Vector2Inversion::Y,
        "/user/hand/left/input/thumbstick",
    );
    for (name, path) in [
        (
            c"/actions/inversion/in/click",
            "/user/hand/left/input/thumbstick/click",
        ),
        (
            c"/actions/inversion/in/touch",
            "/user/hand/left/input/thumbstick/touch",
        ),
    ] {
        f.verify_bindings::<bool>(Knuckles::profile_path(), name, [path.into()]);
        fakexr::set_action_state(
            f.get_action::<bool>(f.get_action_handle(name)),
            ActionState::Bool(true),
            LeftHand,
        );
    }
    let dpad = f.get_action::<xr::Vector2f>(
        f.get_action_handle(c"/user/hand/left/input/thumbstick-/actions/inversion"),
    );
    let activator = f.get_action::<f32>(
        f.get_action_handle(c"/user/hand/left/input/thumbstick/touch-/actions/inversion"),
    );
    assert_ne!(action, dpad);
    fakexr::set_action_state(dpad, ActionState::Vector2(0.0, 0.75), LeftHand);
    fakexr::set_action_state(activator, ActionState::Float(1.0), LeftHand);
    fakexr::set_action_state(action, ActionState::Vector2(0.0, 0.75), LeftHand);
    sync(&mut f);
    check(read(&f, position, left), [0.0, -0.75], [0.0, -0.75], left);
    for name in [
        c"/actions/inversion/in/click",
        c"/actions/inversion/in/touch",
        c"/actions/inversion/in/north",
    ] {
        let state = f
            .get_bool_state_hand(f.get_action_handle(name), left)
            .unwrap();
        assert!(state.bActive && state.bState);
    }
}
