#!/usr/bin/env python
"""
GENERATE POSE TRANSFORMATIONS

Pass this script the left & right `rendermodels/*/*.json` files for a device
and it will print the `match` branches for each `BoundPoseType` for use in your
profile's `pose_transformation` method:

Errors & warnings will get printed to stderr, you may wish to redirect one
output.


    ./generate_pose_transforms.py <left-rendermodels>.json <right-rendermodels>.json

"""

import json
import sys

def main():
    [left_filename, right_filename] = sys.argv[1:3]
    left_json = {}
    right_json = {}
    with open(left_filename) as left_file:
        left_json = json.load(left_file)['components']
    with open(right_filename) as right_file:
        right_json = json.load(right_file)['components']

    pairs = [
        ("raw", "Raw"),
        ("tip", "Tip"),
        ("base", "Base"),
        ("gdc2015", "Gdc2015"),
        ("handgrip", "Handgrip"),
        ("grip", "Grip"),
        ("openxr_handmodel", "OpenxrHandmodel"),
        ("openxr_pinch", "OpenxrPinch"),
        ("openxr_poke", "OpenxrPoke"),
        ("openxr_aim", "OpenxrAim"),
        ("openxr_grip", "OpenxrGrip"),
        ]
    for (json_pose, xrizer_pose) in pairs:
        if (json_pose in left_json) != (json_pose in right_json):
            print(f"Found pair in one file but not other: {json_pose}", file=sys.stderr)
            continue
        if not (json_pose in left_json and json_pose in right_json):
            print(f"            BoundPoseType::{xrizer_pose} => None,")
            continue
        left_pose = left_json[json_pose]['component_local']
        right_pose = right_json[json_pose]['component_local']
        print(f"""
            BoundPoseType::{xrizer_pose} => Some(PoseTransformations {{
                left_hand: Mat4::from_rotation_translation(
                    Quat::from_euler(
                        EulerRot::XYZ,
                        {left_pose['rotate_xyz'][0]}_f32.to_radians(),
                        {left_pose['rotate_xyz'][1]}_f32.to_radians(),
                        {left_pose['rotate_xyz'][2]}_f32.to_radians(),
                    ),
                    Vec3::new({left_pose['origin'][0]}, {left_pose['origin'][1]}, {left_pose['origin'][2]}),
                ),
                right_hand: Mat4::from_rotation_translation(
                    Quat::from_euler(
                        EulerRot::XYZ,
                        {right_pose['rotate_xyz'][0]}_f32.to_radians(),
                        {right_pose['rotate_xyz'][1]}_f32.to_radians(),
                        {right_pose['rotate_xyz'][2]}_f32.to_radians(),
                    ),
                    Vec3::new({right_pose['origin'][0]}, {right_pose['origin'][1]}, {right_pose['origin'][2]}),
                ),
            }}),
              """)


if __name__ == '__main__':
    main()
